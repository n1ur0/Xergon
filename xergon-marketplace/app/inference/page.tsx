"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { cn } from "@/lib/utils";
import {
  Activity,
  Zap,
  Clock,
  Server,
  Brain,
  ArrowRight,
  X,
  Plus,
  TrendingUp,
  CircleDot,
  AlertTriangle,
  CheckCircle2,
  Loader2,
  ChevronDown,
  ExternalLink,
  Cpu,
  BarChart3,
  DollarSign,
  Timer,
  Layers,
  Workflow,
  Play,
  FileText,
  ArrowUpDown,
} from "lucide-react";

// ── Types ──────────────────────────────────────────────────────────────────

type SessionStatus = "active" | "processing" | "completed" | "error" | "queued";
type Strategy = "single" | "cot" | "sharded";

interface CoTStep {
  stepIndex: number;
  label: string;
  provider: string;
  model: string;
  tokensInput: number;
  tokensOutput: number;
  latencyMs: number;
  status: "completed" | "active" | "pending" | "error";
}

interface ProviderContribution {
  provider: string;
  tokensProcessed: number;
  computeTimeMs: number;
  costErg: number;
  color: string;
}

interface InferenceSession {
  id: string;
  model: string;
  strategy: Strategy;
  status: SessionStatus;
  providers: string[];
  totalTokens: number;
  tokensPerMin: number;
  latencyMs: number;
  costErg: number;
  createdAt: Date;
  prompt: string;
  cotSteps: CoTStep[];
  providerContributions: ProviderContribution[];
}

interface InferenceFilters {
  status: string;
  strategy: string;
  model: string;
  sortBy: "newest" | "oldest" | "tokens" | "latency" | "cost";
}

interface NewInferenceForm {
  model: string;
  prompt: string;
  strategy: Strategy;
  maxTokens: number;
}

// ── Provider Colors ────────────────────────────────────────────────────────

const PROVIDER_COLORS: Record<string, string> = {
  NeuralForge: "bg-blue-500",
  GPUHive: "bg-emerald-500",
  TensorNode: "bg-purple-500",
  DeepCompute: "bg-orange-500",
  InferX: "bg-pink-500",
  ComputeHive: "bg-cyan-500",
  ModelServe: "bg-amber-500",
};

const PROVIDER_TEXT: Record<string, string> = {
  NeuralForge: "text-blue-500",
  GPUHive: "text-emerald-500",
  TensorNode: "text-purple-500",
  DeepCompute: "text-orange-500",
  InferX: "text-pink-500",
  ComputeHive: "text-cyan-500",
  ModelServe: "text-amber-500",
};

const PROVIDER_BG: Record<string, string> = {
  NeuralForge: "bg-blue-500/10",
  GPUHive: "bg-emerald-500/10",
  TensorNode: "bg-purple-500/10",
  DeepCompute: "bg-orange-500/10",
  InferX: "bg-pink-500/10",
  ComputeHive: "bg-cyan-500/10",
  ModelServe: "bg-amber-500/10",
};

const PROVIDER_BORDER: Record<string, string> = {
  NeuralForge: "border-blue-500",
  GPUHive: "border-emerald-500",
  TensorNode: "border-purple-500",
  DeepCompute: "border-orange-500",
  InferX: "border-pink-500",
  ComputeHive: "border-cyan-500",
  ModelServe: "border-amber-500",
};

// ── Mock Data ──────────────────────────────────────────────────────────────

const NOW = new Date();

function minutesAgo(m: number): Date {
  return new Date(NOW.getTime() - m * 60000);
}

const MOCK_SESSIONS: InferenceSession[] = [
  {
    id: "inf-001",
    model: "Llama-3.1-70B",
    strategy: "cot",
    status: "active",
    providers: ["NeuralForge", "GPUHive", "TensorNode"],
    totalTokens: 4820,
    tokensPerMin: 342,
    latencyMs: 284,
    costErg: 0.0384,
    createdAt: minutesAgo(14),
    prompt: "Explain the consensus mechanism of Ergo blockchain and compare it with Proof of Work...",
    cotSteps: [
      { stepIndex: 0, label: "Parse Query", provider: "NeuralForge", model: "Llama-3.1-70B", tokensInput: 42, tokensOutput: 18, latencyMs: 45, status: "completed" },
      { stepIndex: 1, label: "Retrieve Knowledge", provider: "GPUHive", model: "Llama-3.1-70B", tokensInput: 128, tokensOutput: 512, latencyMs: 89, status: "completed" },
      { stepIndex: 2, label: "Reason about PoW", provider: "TensorNode", model: "Llama-3.1-70B", tokensInput: 640, tokensOutput: 820, latencyMs: 112, status: "completed" },
      { stepIndex: 3, label: "Analyze Autolykos", provider: "NeuralForge", model: "Llama-3.1-70B", tokensInput: 820, tokensOutput: 960, latencyMs: 98, status: "active" },
      { stepIndex: 4, label: "Compare Mechanisms", provider: "GPUHive", model: "Llama-3.1-70B", tokensInput: 960, tokensOutput: 720, latencyMs: 0, status: "pending" },
      { stepIndex: 5, label: "Synthesize Answer", provider: "TensorNode", model: "Llama-3.1-70B", tokensInput: 720, tokensOutput: 480, latencyMs: 0, status: "pending" },
    ],
    providerContributions: [
      { provider: "NeuralForge", tokensProcessed: 1840, computeTimeMs: 143, costErg: 0.0147, color: PROVIDER_COLORS.NeuralForge },
      { provider: "GPUHive", tokensProcessed: 1360, computeTimeMs: 89, costErg: 0.0109, color: PROVIDER_COLORS.GPUHive },
      { provider: "TensorNode", tokensProcessed: 1620, computeTimeMs: 112, costErg: 0.0130, color: PROVIDER_COLORS.TensorNode },
    ],
  },
  {
    id: "inf-002",
    model: "Qwen-2.5-72B",
    strategy: "sharded",
    status: "processing",
    providers: ["DeepCompute", "InferX"],
    totalTokens: 12400,
    tokensPerMin: 520,
    latencyMs: 145,
    costErg: 0.0992,
    createdAt: minutesAgo(24),
    prompt: "Write a comprehensive technical analysis of transformer architecture improvements since 2017...",
    cotSteps: [
      { stepIndex: 0, label: "Shard 0: Intro", provider: "DeepCompute", model: "Qwen-2.5-72B", tokensInput: 38, tokensOutput: 2400, latencyMs: 68, status: "completed" },
      { stepIndex: 1, label: "Shard 1: Attention", provider: "InferX", model: "Qwen-2.5-72B", tokensInput: 2400, tokensOutput: 3200, latencyMs: 82, status: "completed" },
      { stepIndex: 2, label: "Shard 2: Positional", provider: "DeepCompute", model: "Qwen-2.5-72B", tokensInput: 3200, tokensOutput: 2800, latencyMs: 72, status: "active" },
      { stepIndex: 3, label: "Shard 3: Scaling", provider: "InferX", model: "Qwen-2.5-72B", tokensInput: 2800, tokensOutput: 2400, latencyMs: 0, status: "pending" },
      { stepIndex: 4, label: "Merge & Finalize", provider: "DeepCompute", model: "Qwen-2.5-72B", tokensInput: 2400, tokensOutput: 1600, latencyMs: 0, status: "pending" },
    ],
    providerContributions: [
      { provider: "DeepCompute", tokensProcessed: 6438, computeTimeMs: 140, costErg: 0.0515, color: PROVIDER_COLORS.DeepCompute },
      { provider: "InferX", tokensProcessed: 5962, computeTimeMs: 82, costErg: 0.0477, color: PROVIDER_COLORS.InferX },
    ],
  },
  {
    id: "inf-003",
    model: "Mixtral-8x7B",
    strategy: "single",
    status: "completed",
    providers: ["NeuralForge"],
    totalTokens: 2100,
    tokensPerMin: 450,
    latencyMs: 120,
    costErg: 0.0126,
    createdAt: minutesAgo(5),
    prompt: "What are the key differences between MoE and dense transformer architectures?",
    cotSteps: [
      { stepIndex: 0, label: "Full Inference", provider: "NeuralForge", model: "Mixtral-8x7B", tokensInput: 18, tokensOutput: 2082, latencyMs: 120, status: "completed" },
    ],
    providerContributions: [
      { provider: "NeuralForge", tokensProcessed: 2100, computeTimeMs: 120, costErg: 0.0126, color: PROVIDER_COLORS.NeuralForge },
    ],
  },
  {
    id: "inf-004",
    model: "Llama-3.1-70B",
    strategy: "cot",
    status: "completed",
    providers: ["GPUHive", "DeepCompute", "InferX", "NeuralForge"],
    totalTokens: 8900,
    tokensPerMin: 298,
    latencyMs: 410,
    costErg: 0.0712,
    createdAt: minutesAgo(30),
    prompt: "Design a decentralized GPU marketplace with privacy-preserving compute verification...",
    cotSteps: [
      { stepIndex: 0, label: "Understand Requirements", provider: "GPUHive", model: "Llama-3.1-70B", tokensInput: 24, tokensOutput: 320, latencyMs: 38, status: "completed" },
      { stepIndex: 1, label: "Research Privacy Tech", provider: "DeepCompute", model: "Llama-3.1-70B", tokensInput: 344, tokensOutput: 860, latencyMs: 95, status: "completed" },
      { stepIndex: 2, label: "Design Architecture", provider: "InferX", model: "Llama-3.1-70B", tokensInput: 1204, tokensOutput: 1400, latencyMs: 128, status: "completed" },
      { stepIndex: 3, label: "Plan Verification", provider: "NeuralForge", model: "Llama-3.1-70B", tokensInput: 2604, tokensOutput: 1800, latencyMs: 105, status: "completed" },
      { stepIndex: 4, label: "Draft Implementation", provider: "GPUHive", model: "Llama-3.1-70B", tokensInput: 4404, tokensOutput: 2400, latencyMs: 142, status: "completed" },
      { stepIndex: 5, label: "Final Review", provider: "DeepCompute", model: "Llama-3.1-70B", tokensInput: 6804, tokensOutput: 2096, latencyMs: 88, status: "completed" },
    ],
    providerContributions: [
      { provider: "GPUHive", tokensProcessed: 2724, computeTimeMs: 180, costErg: 0.0218, color: PROVIDER_COLORS.GPUHive },
      { provider: "DeepCompute", tokensProcessed: 2956, computeTimeMs: 183, costErg: 0.0236, color: PROVIDER_COLORS.DeepCompute },
      { provider: "InferX", tokensProcessed: 1400, computeTimeMs: 128, costErg: 0.0112, color: PROVIDER_COLORS.InferX },
      { provider: "NeuralForge", tokensProcessed: 1800, computeTimeMs: 105, costErg: 0.0144, color: PROVIDER_COLORS.NeuralForge },
    ],
  },
  {
    id: "inf-005",
    model: "Qwen-2.5-72B",
    strategy: "single",
    status: "active",
    providers: ["ComputeHive"],
    totalTokens: 3200,
    tokensPerMin: 640,
    latencyMs: 95,
    costErg: 0.0256,
    createdAt: minutesAgo(5),
    prompt: "Summarize the latest advances in quantization techniques for LLM inference...",
    cotSteps: [
      { stepIndex: 0, label: "Full Inference", provider: "ComputeHive", model: "Qwen-2.5-72B", tokensInput: 14, tokensOutput: 3186, latencyMs: 95, status: "active" },
    ],
    providerContributions: [
      { provider: "ComputeHive", tokensProcessed: 3200, computeTimeMs: 95, costErg: 0.0256, color: PROVIDER_COLORS.ComputeHive },
    ],
  },
  {
    id: "inf-006",
    model: "Mixtral-8x7B",
    strategy: "sharded",
    status: "queued",
    providers: ["TensorNode", "NeuralForge", "GPUHive"],
    totalTokens: 0,
    tokensPerMin: 0,
    latencyMs: 0,
    costErg: 0,
    createdAt: minutesAgo(1),
    prompt: "Translate the following medical research paper from German to English with annotations...",
    cotSteps: [
      { stepIndex: 0, label: "Shard 0: Parse", provider: "TensorNode", model: "Mixtral-8x7B", tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
      { stepIndex: 1, label: "Shard 1: Translate", provider: "NeuralForge", model: "Mixtral-8x7B", tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
      { stepIndex: 2, label: "Shard 2: Annotate", provider: "GPUHive", model: "Mixtral-8x7B", tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
      { stepIndex: 3, label: "Merge Output", provider: "TensorNode", model: "Mixtral-8x7B", tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
    ],
    providerContributions: [
      { provider: "TensorNode", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.TensorNode },
      { provider: "NeuralForge", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.NeuralForge },
      { provider: "GPUHive", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.GPUHive },
    ],
  },
  {
    id: "inf-007",
    model: "Llama-3.1-70B",
    strategy: "cot",
    status: "error",
    providers: ["InferX", "DeepCompute"],
    totalTokens: 1560,
    tokensPerMin: 0,
    latencyMs: 0,
    costErg: 0.0125,
    createdAt: minutesAgo(8),
    prompt: "Generate a Python implementation of zero-knowledge proof for range verification...",
    cotSteps: [
      { stepIndex: 0, label: "Parse Request", provider: "InferX", model: "Llama-3.1-70B", tokensInput: 28, tokensOutput: 420, latencyMs: 52, status: "completed" },
      { stepIndex: 1, label: "Design ZK Circuit", provider: "DeepCompute", model: "Llama-3.1-70B", tokensInput: 448, tokensOutput: 680, latencyMs: 78, status: "completed" },
      { stepIndex: 2, label: "Generate Code", provider: "InferX", model: "Llama-3.1-70B", tokensInput: 1128, tokensOutput: 0, latencyMs: 0, status: "error" },
      { stepIndex: 3, label: "Write Tests", provider: "DeepCompute", model: "Llama-3.1-70B", tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
    ],
    providerContributions: [
      { provider: "InferX", tokensProcessed: 870, computeTimeMs: 52, costErg: 0.0070, color: PROVIDER_COLORS.InferX },
      { provider: "DeepCompute", tokensProcessed: 690, computeTimeMs: 78, costErg: 0.0055, color: PROVIDER_COLORS.DeepCompute },
    ],
  },
  {
    id: "inf-008",
    model: "Qwen-2.5-72B",
    strategy: "sharded",
    status: "active",
    providers: ["ModelServe", "ComputeHive"],
    totalTokens: 6800,
    tokensPerMin: 480,
    latencyMs: 168,
    costErg: 0.0544,
    createdAt: minutesAgo(14),
    prompt: "Analyze the economic implications of token-curated registries in decentralized science...",
    cotSteps: [
      { stepIndex: 0, label: "Shard 0: Econ Theory", provider: "ModelServe", model: "Qwen-2.5-72B", tokensInput: 32, tokensOutput: 1800, latencyMs: 58, status: "completed" },
      { stepIndex: 1, label: "Shard 1: DeSci Context", provider: "ComputeHive", model: "Qwen-2.5-72B", tokensInput: 1832, tokensOutput: 2200, latencyMs: 72, status: "completed" },
      { stepIndex: 2, label: "Shard 2: TCR Analysis", provider: "ModelServe", model: "Qwen-2.5-72B", tokensInput: 4032, tokensOutput: 1600, latencyMs: 64, status: "active" },
      { stepIndex: 3, label: "Merge & Synthesize", provider: "ComputeHive", model: "Qwen-2.5-72B", tokensInput: 1600, tokensOutput: 1200, latencyMs: 0, status: "pending" },
    ],
    providerContributions: [
      { provider: "ModelServe", tokensProcessed: 3432, computeTimeMs: 122, costErg: 0.0275, color: PROVIDER_COLORS.ModelServe },
      { provider: "ComputeHive", tokensProcessed: 3368, computeTimeMs: 72, costErg: 0.0269, color: PROVIDER_COLORS.ComputeHive },
    ],
  },
];

const AVAILABLE_MODELS = [
  "Llama-3.1-70B",
  "Qwen-2.5-72B",
  "Mixtral-8x7B",
  "Llama-3.1-8B",
  "Mistral-7B",
];

const DEFAULT_FORM: NewInferenceForm = {
  model: "Llama-3.1-70B",
  prompt: "",
  strategy: "cot",
  maxTokens: 2048,
};

// ── Helpers ────────────────────────────────────────────────────────────────

function StatusBadge({ status }: { status: SessionStatus }) {
  const config: Record<SessionStatus, { label: string; className: string; icon: React.ReactNode }> = {
    active: {
      label: "Active",
      className: "bg-emerald-100 text-emerald-700",
      icon: <CircleDot className="w-3 h-3" />,
    },
    processing: {
      label: "Processing",
      className: "bg-blue-100 text-blue-700",
      icon: <Loader2 className="w-3 h-3 animate-spin" />,
    },
    completed: {
      label: "Completed",
      className: "bg-brand-100 text-brand-700",
      icon: <CheckCircle2 className="w-3 h-3" />,
    },
    error: {
      label: "Error",
      className: "bg-red-100 text-red-700",
      icon: <AlertTriangle className="w-3 h-3" />,
    },
    queued: {
      label: "Queued",
      className: "bg-amber-100 text-amber-700",
      icon: <Clock className="w-3 h-3" />,
    },
  };

  const c = config[status];
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium", c.className)}>
      {c.icon}
      {c.label}
    </span>
  );
}

function StrategyBadge({ strategy }: { strategy: Strategy }) {
  const config: Record<Strategy, { label: string; className: string }> = {
    single: { label: "Single", className: "bg-surface-100 text-surface-800/70" },
    cot: { label: "CoT", className: "bg-purple-100 text-purple-700" },
    sharded: { label: "Sharded", className: "bg-cyan-100 text-cyan-700" },
  };

  const c = config[strategy];
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium", c.className)}>
      {strategy === "cot" && <Workflow className="w-3 h-3" />}
      {strategy === "sharded" && <Layers className="w-3 h-3" />}
      {strategy === "single" && <Cpu className="w-3 h-3" />}
      {c.label}
    </span>
  );
}

function formatTimeAgo(date: Date): string {
  const diffMs = NOW.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.floor(diffHr / 24)}d ago`;
}

function truncatePrompt(prompt: string, maxLen = 50): string {
  if (prompt.length <= maxLen) return prompt;
  return prompt.slice(0, maxLen) + "...";
}

// ── CoT Step Visualization ─────────────────────────────────────────────────

function CoTChain({ steps }: { steps: CoTStep[] }) {
  return (
    <div className="flex flex-col gap-1">
      {steps.map((step, i) => (
        <div key={step.stepIndex} className="flex items-stretch gap-2">
          {/* Step node */}
          <div className="flex flex-col items-center flex-shrink-0">
            <div
              className={cn(
                "w-10 h-10 rounded-full border-2 flex items-center justify-center text-xs font-bold transition-all",
                step.status === "completed" && "border-emerald-500 bg-emerald-500/10 text-emerald-700",
                step.status === "active" && "border-brand-500 bg-brand-500/10 text-brand-700 animate-pulse",
                step.status === "pending" && "border-surface-200 bg-surface-50 text-surface-800/40",
                step.status === "error" && "border-red-500 bg-red-500/10 text-red-700",
              )}
            >
              {step.status === "active" ? (
                <Loader2 className="w-4 h-4 animate-spin" />
              ) : step.status === "completed" ? (
                <CheckCircle2 className="w-4 h-4" />
              ) : step.status === "error" ? (
                <AlertTriangle className="w-4 h-4" />
              ) : (
                step.stepIndex + 1
              )}
            </div>
            {i < steps.length - 1 && (
              <div className="w-0.5 flex-1 bg-surface-200 min-h-[8px]" />
            )}
          </div>

          {/* Step content */}
          <div className={cn(
            "flex-1 rounded-lg border p-2.5 mb-1 transition-all",
            step.status === "active" ? "border-brand-300 bg-brand-50/50" :
            step.status === "error" ? "border-red-200 bg-red-50/50" :
            step.status === "pending" ? "border-surface-100 bg-surface-50/50 opacity-50" :
            "border-surface-100 bg-surface-0"
          )}>
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs font-semibold text-surface-900">{step.label}</span>
              <div className="flex items-center gap-1.5">
                <span className={cn("text-[10px] font-medium", PROVIDER_TEXT[step.provider] ?? "text-surface-800/50")}>
                  {step.provider}
                </span>
              </div>
            </div>
            {step.status !== "pending" && (
              <div className="flex items-center gap-3 text-[10px] text-surface-800/50">
                <span>in: {step.tokensInput}</span>
                <span>out: {step.tokensOutput}</span>
                {step.latencyMs > 0 && <span>{step.latencyMs}ms</span>}
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Provider Load Bar Chart ────────────────────────────────────────────────

function ProviderLoadChart({ sessions }: { sessions: InferenceSession[] }) {
  const providerMap = new Map<string, { tokens: number; sessions: number; cost: number }>();

  for (const session of sessions) {
    if (session.status === "queued") continue;
    for (const contrib of session.providerContributions) {
      const entry = providerMap.get(contrib.provider) ?? { tokens: 0, sessions: 0, cost: 0 };
      entry.tokens += contrib.tokensProcessed;
      entry.cost += contrib.costErg;
      if (!entry.sessions) entry.sessions = 0;
      providerMap.set(contrib.provider, entry);
    }
  }

  // Count active sessions per provider
  for (const session of sessions) {
    if (session.status === "active" || session.status === "processing") {
      for (const provider of session.providers) {
        const entry = providerMap.get(provider);
        if (entry) entry.sessions += 1;
      }
    }
  }

  const maxTokens = Math.max(...Array.from(providerMap.values()).map(v => v.tokens), 1);
  const sortedProviders = Array.from(providerMap.entries()).sort((a, b) => b[1].tokens - a[1].tokens);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="font-semibold text-surface-900 mb-1">Provider Load Distribution</h3>
      <p className="text-xs text-surface-800/40 mb-4">Token processing across active providers</p>
      <div className="space-y-3">
        {sortedProviders.map(([provider, data]) => (
          <div key={provider} className="flex items-center gap-3">
            <div className={cn("w-3 h-3 rounded-sm flex-shrink-0", PROVIDER_COLORS[provider] ?? "bg-surface-300")} />
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between mb-0.5">
                <span className="text-sm font-medium text-surface-900 truncate">{provider}</span>
                <span className="text-xs text-surface-800/40">
                  {data.tokens.toLocaleString()} tok · {data.sessions} active · {data.cost.toFixed(4)} ERG
                </span>
              </div>
              <div className="h-2.5 rounded-full bg-surface-100 overflow-hidden">
                <div
                  className={cn("h-full rounded-full transition-all", PROVIDER_COLORS[provider] ?? "bg-surface-300")}
                  style={{ width: `${(data.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Provider Contribution Breakdown ────────────────────────────────────────

function ContributionBreakdown({ contributions }: { contributions: ProviderContribution[] }) {
  const totalTokens = contributions.reduce((s, c) => s + c.tokensProcessed, 0);
  const totalTime = contributions.reduce((s, c) => s + c.computeTimeMs, 0);
  const totalCost = contributions.reduce((s, c) => s + c.costErg, 0);

  if (totalTokens === 0) {
    return <p className="text-xs text-surface-800/40 text-center py-4">No contributions yet</p>;
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between text-xs text-surface-800/40">
        <span>Provider</span>
        <div className="flex gap-4">
          <span className="w-16 text-right">Tokens</span>
          <span className="w-16 text-right">Time</span>
          <span className="w-16 text-right">Cost</span>
        </div>
      </div>
      {contributions.map((c) => (
        <div key={c.provider} className="flex items-center gap-2">
          <div className={cn("w-2.5 h-2.5 rounded-sm flex-shrink-0", c.color)} />
          <span className="text-sm font-medium text-surface-900 flex-1 truncate">{c.provider}</span>
          <div className="flex gap-4 text-xs text-surface-800/60">
            <span className="w-16 text-right font-mono">{c.tokensProcessed.toLocaleString()}</span>
            <span className="w-16 text-right font-mono">{c.computeTimeMs}ms</span>
            <span className="w-16 text-right font-mono">{c.costErg.toFixed(4)}</span>
          </div>
        </div>
      ))}
      <div className="flex items-center gap-2 pt-2 border-t border-surface-100">
        <div className="w-2.5 h-2.5 flex-shrink-0" />
        <span className="text-sm font-semibold text-surface-900 flex-1">Total</span>
        <div className="flex gap-4 text-xs font-semibold text-surface-900">
          <span className="w-16 text-right font-mono">{totalTokens.toLocaleString()}</span>
          <span className="w-16 text-right font-mono">{totalTime}ms</span>
          <span className="w-16 text-right font-mono">{totalCost.toFixed(4)}</span>
        </div>
      </div>
    </div>
  );
}

// ── Session Detail Modal ───────────────────────────────────────────────────

function SessionDetailModal({
  session,
  isOpen,
  onClose,
}: {
  session: InferenceSession | null;
  isOpen: boolean;
  onClose: () => void;
}) {
  if (!isOpen || !session) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      <div
        className="relative w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl bg-surface-0 p-6 shadow-xl border border-surface-200"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 p-1 rounded-lg hover:bg-surface-100 transition-colors"
        >
          <X className="w-5 h-5 text-surface-800/60" />
        </button>

        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-lg font-bold text-surface-900">{session.model}</h2>
            <div className="flex items-center gap-2 mt-1">
              <span className="text-xs text-surface-800/40 font-mono">{session.id}</span>
              <StatusBadge status={session.status} />
              <StrategyBadge strategy={session.strategy} />
            </div>
          </div>
        </div>

        {/* Prompt */}
        <div className="rounded-lg bg-surface-50 p-3 mb-4">
          <div className="flex items-center gap-1.5 mb-1">
            <FileText className="w-3.5 h-3.5 text-surface-800/40" />
            <span className="text-xs font-medium text-surface-800/40">Prompt</span>
          </div>
          <p className="text-sm text-surface-900">{session.prompt}</p>
        </div>

        {/* Metrics */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-5">
          <div className="rounded-lg bg-surface-50 p-3 text-center">
            <div className="text-lg font-bold text-surface-900">{session.totalTokens.toLocaleString()}</div>
            <div className="text-[10px] text-surface-800/40">Total Tokens</div>
          </div>
          <div className="rounded-lg bg-surface-50 p-3 text-center">
            <div className="text-lg font-bold text-brand-600">{session.tokensPerMin}</div>
            <div className="text-[10px] text-surface-800/40">Tokens/min</div>
          </div>
          <div className="rounded-lg bg-surface-50 p-3 text-center">
            <div className="text-lg font-bold text-surface-900">{session.latencyMs}ms</div>
            <div className="text-[10px] text-surface-800/40">Latency</div>
          </div>
          <div className="rounded-lg bg-surface-50 p-3 text-center">
            <div className="text-lg font-bold text-emerald-600">{session.costErg.toFixed(4)}</div>
            <div className="text-[10px] text-surface-800/40">ERG Cost</div>
          </div>
        </div>

        {/* CoT Chain */}
        <div className="mb-5">
          <h3 className="font-semibold text-surface-900 mb-3 flex items-center gap-2">
            <Workflow className="w-4 h-4" />
            {session.strategy === "cot" ? "Chain-of-Thought Steps" : session.strategy === "sharded" ? "Sharding Pipeline" : "Inference Steps"}
          </h3>
          <CoTChain steps={session.cotSteps} />
        </div>

        {/* Provider Breakdown */}
        <div>
          <h3 className="font-semibold text-surface-900 mb-3 flex items-center gap-2">
            <BarChart3 className="w-4 h-4" />
            Provider Contributions
          </h3>
          <div className="rounded-lg border border-surface-100 p-4">
            <ContributionBreakdown contributions={session.providerContributions} />
          </div>
        </div>
      </div>
    </div>
  );
}

// ── New Inference Form ─────────────────────────────────────────────────────

function NewInferenceForm({
  onSubmit,
  isSubmitting,
}: {
  onSubmit: (form: NewInferenceForm) => void;
  isSubmitting: boolean;
}) {
  const [form, setForm] = useState<NewInferenceForm>(DEFAULT_FORM);
  const [showAdvanced, setShowAdvanced] = useState(false);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!form.prompt.trim()) return;
    onSubmit(form);
  }

  return (
    <form onSubmit={handleSubmit} className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="font-semibold text-surface-900 mb-4 flex items-center gap-2">
        <Plus className="w-4 h-4" />
        New Inference
      </h3>

      <div className="space-y-4">
        {/* Model */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Model</label>
          <select
            value={form.model}
            onChange={(e) => setForm((f) => ({ ...f, model: e.target.value }))}
            className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
          >
            {AVAILABLE_MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>

        {/* Prompt */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Prompt</label>
          <textarea
            value={form.prompt}
            onChange={(e) => setForm((f) => ({ ...f, prompt: e.target.value }))}
            placeholder="Enter your inference prompt..."
            rows={3}
            className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 resize-none"
          />
        </div>

        {/* Strategy */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Strategy</label>
          <div className="flex gap-2">
            {(["single", "cot", "sharded"] as const).map((s) => (
              <button
                key={s}
                type="button"
                onClick={() => setForm((f) => ({ ...f, strategy: s }))}
                className={cn(
                  "flex-1 rounded-lg border px-3 py-2 text-xs font-medium transition-all",
                  form.strategy === s
                    ? "border-brand-500 bg-brand-50 text-brand-700"
                    : "border-surface-200 bg-surface-50 text-surface-800/60 hover:border-surface-300"
                )}
              >
                <div className="flex items-center justify-center gap-1">
                  {s === "single" && <Cpu className="w-3 h-3" />}
                  {s === "cot" && <Workflow className="w-3 h-3" />}
                  {s === "sharded" && <Layers className="w-3 h-3" />}
                  <span className="capitalize">{s}</span>
                </div>
              </button>
            ))}
          </div>
        </div>

        {/* Advanced Options */}
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="flex items-center gap-1 text-xs text-surface-800/50 hover:text-surface-800/70 transition-colors"
        >
          <ChevronDown className={cn("w-3 h-3 transition-transform", showAdvanced && "rotate-180")} />
          Advanced Options
        </button>

        {showAdvanced && (
          <div className="rounded-lg border border-surface-100 bg-surface-50 p-3">
            <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Max Tokens</label>
            <input
              type="number"
              value={form.maxTokens}
              onChange={(e) => setForm((f) => ({ ...f, maxTokens: parseInt(e.target.value) || 2048 }))}
              min={64}
              max={32768}
              step={256}
              className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 font-mono focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
            />
            <div className="flex gap-2 mt-2">
              {[512, 1024, 2048, 4096].map((v) => (
                <button
                  key={v}
                  type="button"
                  onClick={() => setForm((f) => ({ ...f, maxTokens: v }))}
                  className={cn(
                    "rounded-md px-2 py-1 text-[10px] font-medium transition-all",
                    form.maxTokens === v
                      ? "bg-brand-100 text-brand-700"
                      : "bg-surface-100 text-surface-800/50 hover:bg-surface-200"
                  )}
                >
                  {v}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Submit */}
        <button
          type="submit"
          disabled={!form.prompt.trim() || isSubmitting}
          className={cn(
            "w-full rounded-lg px-4 py-2.5 text-sm font-semibold transition-all flex items-center justify-center gap-2",
            form.prompt.trim() && !isSubmitting
              ? "bg-brand-600 text-white hover:bg-brand-700 active:scale-[0.98] shadow-sm hover:shadow-md"
              : "bg-surface-200 text-surface-800/40 cursor-not-allowed"
          )}
        >
          {isSubmitting ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Starting...
            </>
          ) : (
            <>
              <Play className="w-4 h-4" />
              Start Inference
            </>
          )}
        </button>
      </div>
    </form>
  );
}

// ── Metric Card ────────────────────────────────────────────────────────────

function MetricCard({
  label,
  value,
  icon,
  trend,
}: {
  label: string;
  value: string;
  icon: React.ReactNode;
  trend?: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 transition-all hover:shadow-md">
      <div className="flex items-start justify-between mb-2">
        <div className="rounded-lg bg-brand-50 p-2 text-brand-600">
          {icon}
        </div>
        {trend && (
          <span className={cn(
            "text-xs font-medium",
            trend.startsWith("+") ? "text-emerald-600" : "text-surface-800/40"
          )}>
            {trend}
          </span>
        )}
      </div>
      <div className="text-2xl font-bold text-surface-900">{value}</div>
      <div className="text-xs text-surface-800/40 mt-0.5">{label}</div>
    </div>
  );
}

// ── Main Page ──────────────────────────────────────────────────────────────

export default function InferencePage() {
  const [sessions, setSessions] = useState<InferenceSession[]>(MOCK_SESSIONS);
  const [selectedSession, setSelectedSession] = useState<InferenceSession | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [filters, setFilters] = useState<InferenceFilters>({
    status: "all",
    strategy: "all",
    model: "all",
    sortBy: "newest",
  });

  // Simulate live metrics
  useEffect(() => {
    const interval = setInterval(() => {
      setSessions((prev) =>
        prev.map((s) => {
          if (s.status !== "active" && s.status !== "processing") return s;
          const delta = Math.round((Math.random() - 0.3) * 20);
          return {
            ...s,
            totalTokens: Math.max(0, s.totalTokens + delta),
            tokensPerMin: Math.max(100, s.tokensPerMin + Math.round((Math.random() - 0.5) * 30)),
            latencyMs: Math.max(50, s.latencyMs + Math.round((Math.random() - 0.5) * 15)),
            costErg: Math.max(0, s.costErg + delta * 0.000008),
          };
        })
      );
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  // Filtered & sorted sessions
  const filteredSessions = useMemo(() => {
    let result = sessions.filter((s) => {
      if (filters.status !== "all" && s.status !== filters.status) return false;
      if (filters.strategy !== "all" && s.strategy !== filters.strategy) return false;
      if (filters.model !== "all" && s.model !== filters.model) return false;
      return true;
    });

    result.sort((a, b) => {
      switch (filters.sortBy) {
        case "newest": return b.createdAt.getTime() - a.createdAt.getTime();
        case "oldest": return a.createdAt.getTime() - b.createdAt.getTime();
        case "tokens": return b.totalTokens - a.totalTokens;
        case "latency": return b.latencyMs - a.latencyMs;
        case "cost": return b.costErg - a.costErg;
        default: return 0;
      }
    });

    return result;
  }, [sessions, filters]);

  // Aggregate metrics
  const metrics = useMemo(() => {
    const activeSessions = sessions.filter((s) => s.status === "active" || s.status === "processing");
    const totalTokensPerMin = activeSessions.reduce((s, sess) => s + sess.tokensPerMin, 0);
    const avgLatency = activeSessions.length > 0
      ? Math.round(activeSessions.reduce((s, sess) => s + sess.latencyMs, 0) / activeSessions.length)
      : 0;
    const totalCost = sessions.reduce((s, sess) => s + sess.costErg, 0);
    return {
      activeSessions: activeSessions.length,
      totalTokensPerMin,
      avgLatency,
      totalCost,
    };
  }, [sessions]);

  const handleNewInference = useCallback((form: NewInferenceForm) => {
    setIsSubmitting(true);
    setTimeout(() => {
      const newSession: InferenceSession = {
        id: `inf-${String(sessions.length + 1).padStart(3, "0")}`,
        model: form.model,
        strategy: form.strategy,
        status: "queued",
        providers: form.strategy === "single"
          ? ["NeuralForge"]
          : form.strategy === "cot"
          ? ["NeuralForge", "GPUHive", "TensorNode"].slice(0, 2 + Math.floor(Math.random() * 2))
          : ["DeepCompute", "InferX", "ComputeHive"].slice(0, 2),
        totalTokens: 0,
        tokensPerMin: 0,
        latencyMs: 0,
        costErg: 0,
        createdAt: new Date(),
        prompt: form.prompt,
        cotSteps: form.strategy === "single"
          ? [{ stepIndex: 0, label: "Full Inference", provider: "NeuralForge", model: form.model, tokensInput: form.prompt.length, tokensOutput: 0, latencyMs: 0, status: "pending" }]
          : form.strategy === "cot"
          ? [
              { stepIndex: 0, label: "Parse Query", provider: "NeuralForge", model: form.model, tokensInput: form.prompt.length, tokensOutput: 0, latencyMs: 0, status: "pending" },
              { stepIndex: 1, label: "Retrieve Context", provider: "GPUHive", model: form.model, tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
              { stepIndex: 2, label: "Reason", provider: "TensorNode", model: form.model, tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
              { stepIndex: 3, label: "Generate Response", provider: "NeuralForge", model: form.model, tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
            ]
          : [
              { stepIndex: 0, label: "Shard 0", provider: "DeepCompute", model: form.model, tokensInput: form.prompt.length, tokensOutput: 0, latencyMs: 0, status: "pending" },
              { stepIndex: 1, label: "Shard 1", provider: "InferX", model: form.model, tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
              { stepIndex: 2, label: "Merge", provider: "DeepCompute", model: form.model, tokensInput: 0, tokensOutput: 0, latencyMs: 0, status: "pending" },
            ],
        providerContributions: form.strategy === "single"
          ? [{ provider: "NeuralForge", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.NeuralForge }]
          : [
              { provider: "NeuralForge", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.NeuralForge },
              { provider: "GPUHive", tokensProcessed: 0, computeTimeMs: 0, costErg: 0, color: PROVIDER_COLORS.GPUHive },
            ],
      };
      setSessions((prev) => [newSession, ...prev]);
      setIsSubmitting(false);
    }, 1000);
  }, [sessions.length]);

  const uniqueModels = [...new Set(sessions.map((s) => s.model))];

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <div className="flex items-center justify-between mb-2">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 flex items-center gap-2">
            <Brain className="w-6 h-6 text-brand-500" />
            Cross-Provider Inference
          </h1>
          <p className="text-surface-800/60 text-sm mt-1">
            Route inference requests across multiple GPU providers with chain-of-thought orchestration.
          </p>
        </div>
      </div>

      {/* Metric Cards */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6 mt-4">
        <MetricCard
          label="Active Sessions"
          value={String(metrics.activeSessions)}
          icon={<Activity className="w-4 h-4" />}
          trend={`of ${sessions.length} total`}
        />
        <MetricCard
          label="Tokens/min"
          value={metrics.totalTokensPerMin.toLocaleString()}
          icon={<Zap className="w-4 h-4" />}
        />
        <MetricCard
          label="Avg Latency"
          value={`${metrics.avgLatency}ms`}
          icon={<Timer className="w-4 h-4" />}
        />
        <MetricCard
          label="ERG Spent"
          value={metrics.totalCost.toFixed(4)}
          icon={<DollarSign className="w-4 h-4" />}
        />
      </div>

      {/* Main Layout */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Sessions Table */}
        <div className="lg:col-span-2">
          {/* Filters */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 mb-4">
            <div className="flex flex-wrap items-center gap-3">
              {/* Status filter */}
              <select
                value={filters.status}
                onChange={(e) => setFilters((f) => ({ ...f, status: e.target.value }))}
                className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                <option value="all">All Status</option>
                <option value="active">Active</option>
                <option value="processing">Processing</option>
                <option value="completed">Completed</option>
                <option value="error">Error</option>
                <option value="queued">Queued</option>
              </select>

              {/* Strategy filter */}
              <select
                value={filters.strategy}
                onChange={(e) => setFilters((f) => ({ ...f, strategy: e.target.value }))}
                className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                <option value="all">All Strategies</option>
                <option value="single">Single</option>
                <option value="cot">CoT</option>
                <option value="sharded">Sharded</option>
              </select>

              {/* Model filter */}
              <select
                value={filters.model}
                onChange={(e) => setFilters((f) => ({ ...f, model: e.target.value }))}
                className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                <option value="all">All Models</option>
                {uniqueModels.map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>

              {/* Sort */}
              <div className="flex items-center gap-1 ml-auto">
                <ArrowUpDown className="w-3 h-3 text-surface-800/40" />
                <select
                  value={filters.sortBy}
                  onChange={(e) => setFilters((f) => ({ ...f, sortBy: e.target.value as InferenceFilters["sortBy"] }))}
                  className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
                >
                  <option value="newest">Newest</option>
                  <option value="oldest">Oldest</option>
                  <option value="tokens">Most Tokens</option>
                  <option value="latency">Highest Latency</option>
                  <option value="cost">Highest Cost</option>
                </select>
              </div>
            </div>
          </div>

          {/* Sessions Table */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-surface-100">
                    <th className="text-left px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Session</th>
                    <th className="text-left px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Model</th>
                    <th className="text-left px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Strategy</th>
                    <th className="text-left px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Status</th>
                    <th className="text-left px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Providers</th>
                    <th className="text-right px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Tokens</th>
                    <th className="text-right px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Latency</th>
                    <th className="text-right px-4 py-3 text-xs font-medium text-surface-800/40 uppercase tracking-wide">Cost</th>
                    <th className="px-4 py-3"></th>
                  </tr>
                </thead>
                <tbody>
                  {filteredSessions.map((session) => (
                    <tr
                      key={session.id}
                      className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors cursor-pointer"
                      onClick={() => {
                        setSelectedSession(session);
                        setIsModalOpen(true);
                      }}
                    >
                      <td className="px-4 py-3">
                        <div className="text-xs font-mono text-surface-800/50">{session.id}</div>
                        <div className="text-[10px] text-surface-800/30 mt-0.5">{formatTimeAgo(session.createdAt)}</div>
                      </td>
                      <td className="px-4 py-3">
                        <span className="text-sm font-medium text-surface-900">{session.model}</span>
                      </td>
                      <td className="px-4 py-3">
                        <StrategyBadge strategy={session.strategy} />
                      </td>
                      <td className="px-4 py-3">
                        <StatusBadge status={session.status} />
                      </td>
                      <td className="px-4 py-3">
                        <div className="flex items-center gap-1">
                          {session.providers.map((p, i) => (
                            <div
                              key={p}
                              className={cn("w-2 h-2 rounded-full", PROVIDER_COLORS[p] ?? "bg-surface-300")}
                              title={p}
                            />
                          ))}
                          <span className="text-xs text-surface-800/40 ml-1">{session.providers.length}</span>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-right">
                        <span className="text-sm font-mono text-surface-900">{session.totalTokens.toLocaleString()}</span>
                        {session.tokensPerMin > 0 && (
                          <div className="text-[10px] text-surface-800/30">{session.tokensPerMin}/min</div>
                        )}
                      </td>
                      <td className="px-4 py-3 text-right">
                        <span className="text-sm font-mono text-surface-900">{session.latencyMs > 0 ? `${session.latencyMs}ms` : "—"}</span>
                      </td>
                      <td className="px-4 py-3 text-right">
                        <span className="text-sm font-mono text-surface-900">{session.costErg > 0 ? `${session.costErg.toFixed(4)}` : "—"}</span>
                      </td>
                      <td className="px-4 py-3">
                        <ExternalLink className="w-3.5 h-3.5 text-surface-800/30" />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {filteredSessions.length === 0 && (
              <div className="text-center py-12 text-sm text-surface-800/40">
                No sessions match the current filters.
              </div>
            )}
          </div>
        </div>

        {/* Right Sidebar */}
        <div className="space-y-6">
          <NewInferenceForm onSubmit={handleNewInference} isSubmitting={isSubmitting} />
          <ProviderLoadChart sessions={sessions} />
        </div>
      </div>

      {/* CoT Step Visualization for active sessions */}
      <div className="mt-6">
        <h2 className="text-lg font-semibold text-surface-900 mb-4 flex items-center gap-2">
          <Workflow className="w-5 h-5 text-brand-500" />
          Active Inference Chains
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {sessions
            .filter((s) => s.status === "active" || s.status === "processing")
            .map((session) => (
              <div
                key={session.id}
                className="rounded-xl border border-surface-200 bg-surface-0 p-4 hover:shadow-md transition-all cursor-pointer"
                onClick={() => {
                  setSelectedSession(session);
                  setIsModalOpen(true);
                }}
              >
                <div className="flex items-center justify-between mb-3">
                  <div>
                    <div className="text-sm font-semibold text-surface-900">{session.model}</div>
                    <div className="text-[10px] font-mono text-surface-800/40">{session.id}</div>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <StatusBadge status={session.status} />
                    <StrategyBadge strategy={session.strategy} />
                  </div>
                </div>

                <p className="text-xs text-surface-800/50 mb-3 line-clamp-1">{truncatePrompt(session.prompt, 60)}</p>

                <CoTChain steps={session.cotSteps} />

                <div className="flex items-center justify-between mt-3 pt-2 border-t border-surface-100 text-[10px] text-surface-800/40">
                  <span>{session.totalTokens.toLocaleString()} tokens</span>
                  <span>{session.latencyMs}ms</span>
                  <span>{session.costErg.toFixed(4)} ERG</span>
                </div>
              </div>
            ))}
        </div>
      </div>

      {/* Session Detail Modal */}
      <SessionDetailModal
        session={selectedSession}
        isOpen={isModalOpen}
        onClose={() => {
          setIsModalOpen(false);
          setSelectedSession(null);
        }}
      />
    </div>
  );
}
