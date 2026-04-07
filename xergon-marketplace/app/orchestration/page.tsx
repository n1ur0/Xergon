"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

type ProviderStatus = "online" | "offline" | "degraded";
type SessionStatus = "running" | "queued" | "completed" | "failed";
type Strategy = "pipeline" | "tensor" | "hybrid";

interface Provider {
  id: string;
  name: string;
  status: ProviderStatus;
  latencyMs: number;
  gpuType: string;
  region: string;
  utilization: number;
  color: string;
}

interface SessionStage {
  stageIndex: number;
  label: string;
  provider: string;
  latencyMs: number;
  status: "active" | "waiting" | "complete" | "error";
  tokensPerSec: number;
}

interface InferenceSession {
  id: string;
  modelName: string;
  strategy: Strategy;
  providers: string[];
  status: SessionStatus;
  progress: number;
  totalTokens: number;
  tokensPerSec: number;
  avgLatencyMs: number;
  createdAt: string;
  stages: SessionStage[];
}

interface CoTRoute {
  step: number;
  label: string;
  provider: string;
  latencyMs: number;
  tokensProcessed: number;
}

interface HistoryEntry {
  id: string;
  modelName: string;
  strategy: Strategy;
  providers: string[];
  status: SessionStatus;
  totalTokens: number;
  avgLatencyMs: number;
  tokensPerSec: number;
  duration: string;
  createdAt: string;
}

// ── Mock Data ──

const PROVIDERS: Provider[] = [
  { id: "nf", name: "NeuralForge", status: "online", latencyMs: 42, gpuType: "A100 80GB", region: "US-East", utilization: 78, color: "#3b82f6" },
  { id: "gh", name: "GPUHive", status: "online", latencyMs: 55, gpuType: "H100 80GB", region: "EU-West", utilization: 65, color: "#10b981" },
  { id: "tn", name: "TensorNode", status: "degraded", latencyMs: 120, gpuType: "A100 40GB", region: "AP-South", utilization: 42, color: "#8b5cf6" },
  { id: "dc", name: "DeepCompute", status: "online", latencyMs: 38, gpuType: "H100 80GB", region: "US-West", utilization: 88, color: "#f97316" },
  { id: "ix", name: "InferX", status: "offline", latencyMs: 0, gpuType: "A100 80GB", region: "EU-Central", utilization: 0, color: "#ec4899" },
  { id: "aq", name: "AquaNet", status: "online", latencyMs: 67, gpuType: "L40S 48GB", region: "US-Central", utilization: 53, color: "#06b6d4" },
];

const ACTIVE_SESSIONS: InferenceSession[] = [
  {
    id: "sess-001",
    modelName: "Llama-3.1-70B",
    strategy: "pipeline",
    providers: ["NeuralForge", "GPUHive", "DeepCompute"],
    status: "running",
    progress: 72,
    totalTokens: 184320,
    tokensPerSec: 128,
    avgLatencyMs: 89,
    createdAt: "2026-04-06T14:23:00Z",
    stages: [
      { stageIndex: 0, label: "Embedding", provider: "NeuralForge", latencyMs: 18, status: "complete", tokensPerSec: 142 },
      { stageIndex: 1, label: "Layers 0-23", provider: "NeuralForge", latencyMs: 34, status: "complete", tokensPerSec: 138 },
      { stageIndex: 2, label: "Layers 24-47", provider: "GPUHive", latencyMs: 42, status: "active", tokensPerSec: 125 },
      { stageIndex: 3, label: "Layers 48-79", provider: "DeepCompute", latencyMs: 28, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 4, label: "LM Head", provider: "DeepCompute", latencyMs: 12, status: "waiting", tokensPerSec: 0 },
    ],
  },
  {
    id: "sess-002",
    modelName: "Mixtral-8x22B",
    strategy: "tensor",
    providers: ["DeepCompute", "GPUHive", "AquaNet", "NeuralForge"],
    status: "running",
    progress: 45,
    totalTokens: 92160,
    tokensPerSec: 96,
    avgLatencyMs: 112,
    createdAt: "2026-04-06T14:28:00Z",
    stages: [
      { stageIndex: 0, label: "Experts 0-1", provider: "DeepCompute", latencyMs: 28, status: "active", tokensPerSec: 98 },
      { stageIndex: 1, label: "Experts 2-3", provider: "GPUHive", latencyMs: 35, status: "active", tokensPerSec: 94 },
      { stageIndex: 2, label: "Experts 4-5", provider: "AquaNet", latencyMs: 52, status: "active", tokensPerSec: 88 },
      { stageIndex: 3, label: "Experts 6-7", provider: "NeuralForge", latencyMs: 22, status: "active", tokensPerSec: 102 },
    ],
  },
  {
    id: "sess-003",
    modelName: "Qwen2-72B",
    strategy: "hybrid",
    providers: ["NeuralForge", "DeepCompute", "GPUHive", "TensorNode"],
    status: "running",
    progress: 31,
    totalTokens: 65536,
    tokensPerSec: 78,
    avgLatencyMs: 145,
    createdAt: "2026-04-06T14:31:00Z",
    stages: [
      { stageIndex: 0, label: "Embed+Layers 0-11", provider: "NeuralForge", latencyMs: 30, status: "complete", tokensPerSec: 82 },
      { stageIndex: 1, label: "Layers 12-35 (TPx2)", provider: "DeepCompute", latencyMs: 48, status: "active", tokensPerSec: 75 },
      { stageIndex: 2, label: "Layers 36-59 (TPx2)", provider: "GPUHive", latencyMs: 55, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 3, label: "Layers 60-79", provider: "TensorNode", latencyMs: 68, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 4, label: "LM Head", provider: "TensorNode", latencyMs: 15, status: "waiting", tokensPerSec: 0 },
    ],
  },
  {
    id: "sess-004",
    modelName: "DeepSeek-V3",
    strategy: "pipeline",
    providers: ["DeepCompute", "NeuralForge"],
    status: "queued",
    progress: 0,
    totalTokens: 0,
    tokensPerSec: 0,
    avgLatencyMs: 0,
    createdAt: "2026-04-06T14:35:00Z",
    stages: [
      { stageIndex: 0, label: "Embedding", provider: "DeepCompute", latencyMs: 0, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 1, label: "MoE Layers 0-30", provider: "DeepCompute", latencyMs: 0, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 2, label: "MoE Layers 31-60", provider: "NeuralForge", latencyMs: 0, status: "waiting", tokensPerSec: 0 },
      { stageIndex: 3, label: "LM Head", provider: "NeuralForge", latencyMs: 0, status: "waiting", tokensPerSec: 0 },
    ],
  },
  {
    id: "sess-005",
    modelName: "Llama-3.1-8B",
    strategy: "pipeline",
    providers: ["AquaNet"],
    status: "completed",
    progress: 100,
    totalTokens: 32768,
    tokensPerSec: 210,
    avgLatencyMs: 32,
    createdAt: "2026-04-06T14:10:00Z",
    stages: [
      { stageIndex: 0, label: "Embed+Layers 0-31", provider: "AquaNet", latencyMs: 22, status: "complete", tokensPerSec: 215 },
      { stageIndex: 1, label: "Layers 32-63", provider: "AquaNet", latencyMs: 20, status: "complete", tokensPerSec: 208 },
      { stageIndex: 2, label: "LM Head", provider: "AquaNet", latencyMs: 8, status: "complete", tokensPerSec: 220 },
    ],
  },
  {
    id: "sess-006",
    modelName: "Mistral-7B",
    strategy: "tensor",
    providers: ["GPUHive", "InferX"],
    status: "failed",
    progress: 18,
    totalTokens: 12288,
    tokensPerSec: 0,
    avgLatencyMs: 340,
    createdAt: "2026-04-06T14:15:00Z",
    stages: [
      { stageIndex: 0, label: "Attn 0-15", provider: "GPUHive", latencyMs: 25, status: "error", tokensPerSec: 0 },
      { stageIndex: 1, label: "Attn 16-31", provider: "InferX", latencyMs: 0, status: "error", tokensPerSec: 0 },
    ],
  },
];

const COT_ROUTES: CoTRoute[] = [
  { step: 0, label: "Prompt Classifier", provider: "NeuralForge", latencyMs: 12, tokensProcessed: 256 },
  { step: 1, label: "Reasoning Router", provider: "DeepCompute", latencyMs: 8, tokensProcessed: 128 },
  { step: 2, label: "CoT Stage 1", provider: "GPUHive", latencyMs: 85, tokensProcessed: 2048 },
  { step: 3, label: "CoT Stage 2", provider: "NeuralForge", latencyMs: 72, tokensProcessed: 1536 },
  { step: 4, label: "Aggregation", provider: "DeepCompute", latencyMs: 15, tokensProcessed: 512 },
  { step: 5, label: "Response Gen", provider: "AquaNet", latencyMs: 45, tokensProcessed: 768 },
];

const HISTORY: HistoryEntry[] = [
  { id: "h-001", modelName: "Llama-3.1-70B", strategy: "pipeline", providers: ["NeuralForge", "GPUHive", "DeepCompute"], status: "completed", totalTokens: 262144, avgLatencyMs: 85, tokensPerSec: 135, duration: "32m 18s", createdAt: "2026-04-05T10:00:00Z" },
  { id: "h-002", modelName: "Mixtral-8x22B", strategy: "tensor", providers: ["DeepCompute", "GPUHive", "AquaNet"], status: "completed", totalTokens: 196608, avgLatencyMs: 108, tokensPerSec: 92, duration: "35m 42s", createdAt: "2026-04-05T11:30:00Z" },
  { id: "h-003", modelName: "Qwen2-72B", strategy: "hybrid", providers: ["NeuralForge", "DeepCompute"], status: "failed", totalTokens: 45056, avgLatencyMs: 210, tokensPerSec: 0, duration: "8m 05s", createdAt: "2026-04-05T14:00:00Z" },
  { id: "h-004", modelName: "Llama-3.1-8B", strategy: "pipeline", providers: ["AquaNet"], status: "completed", totalTokens: 65536, avgLatencyMs: 30, tokensPerSec: 220, duration: "4m 58s", createdAt: "2026-04-06T09:00:00Z" },
  { id: "h-005", modelName: "DeepSeek-V3", strategy: "pipeline", providers: ["DeepCompute", "GPUHive", "NeuralForge"], status: "completed", totalTokens: 131072, avgLatencyMs: 95, tokensPerSec: 110, duration: "19m 52s", createdAt: "2026-04-06T12:00:00Z" },
  { id: "h-006", modelName: "Mistral-7B", strategy: "tensor", providers: ["GPUHive", "InferX"], status: "failed", totalTokens: 8192, avgLatencyMs: 340, tokensPerSec: 0, duration: "2m 11s", createdAt: "2026-04-06T13:00:00Z" },
  { id: "h-007", modelName: "Mixtral-8x7B", strategy: "hybrid", providers: ["NeuralForge", "AquaNet"], status: "completed", totalTokens: 98304, avgLatencyMs: 72, tokensPerSec: 148, duration: "11m 04s", createdAt: "2026-04-06T13:30:00Z" },
];

const AVAILABLE_MODELS = [
  "Llama-3.1-8B",
  "Llama-3.1-70B",
  "Mixtral-8x7B",
  "Mixtral-8x22B",
  "Qwen2-72B",
  "DeepSeek-V3",
  "Mistral-7B",
];

const PROVIDER_NAMES = PROVIDERS.map(p => p.name);

// ── Helpers ──

function providerColor(name: string): string {
  return PROVIDERS.find(p => p.name === name)?.color ?? "#6b7280";
}

function providerBgClass(name: string): string {
  const map: Record<string, string> = {
    NeuralForge: "bg-blue-500/10",
    GPUHive: "bg-emerald-500/10",
    TensorNode: "bg-purple-500/10",
    DeepCompute: "bg-orange-500/10",
    InferX: "bg-pink-500/10",
    AquaNet: "bg-cyan-500/10",
  };
  return map[name] ?? "bg-gray-500/10";
}

function providerBorderClass(name: string): string {
  const map: Record<string, string> = {
    NeuralForge: "border-blue-500",
    GPUHive: "border-emerald-500",
    TensorNode: "border-purple-500",
    DeepCompute: "border-orange-500",
    InferX: "border-pink-500",
    AquaNet: "border-cyan-500",
  };
  return map[name] ?? "border-gray-500";
}

function providerTextClass(name: string): string {
  const map: Record<string, string> = {
    NeuralForge: "text-blue-600",
    GPUHive: "text-emerald-600",
    TensorNode: "text-purple-600",
    DeepCompute: "text-orange-600",
    InferX: "text-pink-600",
    AquaNet: "text-cyan-600",
  };
  return map[name] ?? "text-gray-600";
}

function statusDot(status: ProviderStatus) {
  switch (status) {
    case "online":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse" />;
    case "degraded":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-yellow-500 animate-pulse" />;
    case "offline":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-red-500" />;
  }
}

function sessionStatusBadge(status: SessionStatus) {
  switch (status) {
    case "running":
      return { text: "Running", cls: "text-emerald-700 bg-emerald-100 dark:text-emerald-300 dark:bg-emerald-900/40" };
    case "queued":
      return { text: "Queued", cls: "text-yellow-700 bg-yellow-100 dark:text-yellow-300 dark:bg-yellow-900/40" };
    case "completed":
      return { text: "Completed", cls: "text-blue-700 bg-blue-100 dark:text-blue-300 dark:bg-blue-900/40" };
    case "failed":
      return { text: "Failed", cls: "text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/40" };
  }
}

function stageStatusIndicator(status: SessionStage["status"]) {
  switch (status) {
    case "active":
      return <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />;
    case "complete":
      return <span className="w-2 h-2 rounded-full bg-blue-500" />;
    case "waiting":
      return <span className="w-2 h-2 rounded-full bg-gray-300 dark:bg-gray-600" />;
    case "error":
      return <span className="w-2 h-2 rounded-full bg-red-500" />;
  }
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

// ── Provider Status Card ──

function ProviderStatusCard({ provider }: { provider: Provider }) {
  return (
    <div className={cn(
      "rounded-xl border p-4 transition-all hover:shadow-md",
      provider.status === "online" ? "border-surface-200 bg-surface-0" :
      provider.status === "degraded" ? "border-yellow-300 bg-yellow-50/50 dark:border-yellow-800 dark:bg-yellow-900/10" :
      "border-red-200 bg-red-50/50 dark:border-red-800 dark:bg-red-900/10 opacity-70"
    )}>
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          {statusDot(provider.status)}
          <span className="font-semibold text-sm text-surface-900 dark:text-surface-100">{provider.name}</span>
        </div>
        <span className={cn(
          "text-[10px] font-medium uppercase tracking-wide px-2 py-0.5 rounded-full",
          provider.status === "online" ? "text-emerald-700 bg-emerald-100 dark:text-emerald-300 dark:bg-emerald-900/40" :
          provider.status === "degraded" ? "text-yellow-700 bg-yellow-100 dark:text-yellow-300 dark:bg-yellow-900/40" :
          "text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/40"
        )}>
          {provider.status}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-surface-800/50 dark:text-surface-300/60">
        <span>GPU: {provider.gpuType}</span>
        <span>Region: {provider.region}</span>
        <span>Latency: {provider.latencyMs > 0 ? `${provider.latencyMs}ms` : "N/A"}</span>
        <span>Util: {provider.utilization}%</span>
      </div>
      {provider.status !== "offline" && (
        <div className="mt-2 h-1.5 rounded-full bg-surface-200 dark:bg-surface-700 overflow-hidden">
          <div
            className={cn(
              "h-full rounded-full transition-all",
              provider.utilization > 80 ? "bg-red-400" :
              provider.utilization > 60 ? "bg-yellow-400" : "bg-emerald-500"
            )}
            style={{ width: `${provider.utilization}%` }}
          />
        </div>
      )}
    </div>
  );
}

// ── Session Card ──

function SessionCard({ session, isSelected, onClick }: { session: InferenceSession; isSelected: boolean; onClick: () => void }) {
  const badge = sessionStatusBadge(session.status);
  return (
    <button
      className={cn(
        "rounded-xl border p-4 text-left transition-all hover:shadow-md w-full",
        isSelected ? "border-brand-500 ring-2 ring-brand-500/20" : "border-surface-200 bg-surface-0 dark:border-surface-700",
        session.status === "failed" && "border-red-200 dark:border-red-800"
      )}
      onClick={onClick}
    >
      <div className="flex items-center justify-between mb-2">
        <span className="font-semibold text-sm text-surface-900 dark:text-surface-100">{session.modelName}</span>
        <span className={cn("rounded-full px-2 py-0.5 text-[10px] font-medium", badge.cls)}>{badge.text}</span>
      </div>
      <div className="flex items-center gap-2 mb-3">
        <span className={cn(
          "rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide",
          session.strategy === "pipeline" ? "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300" :
          session.strategy === "tensor" ? "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300" :
          "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
        )}>
          {session.strategy}
        </span>
        <div className="flex -space-x-1">
          {session.providers.map(p => (
            <span
              key={p}
              className="inline-flex items-center justify-center w-5 h-5 rounded-full text-[8px] font-bold text-white border border-white dark:border-surface-900"
              style={{ backgroundColor: providerColor(p) }}
              title={p}
            >
              {p[0]}
            </span>
          ))}
        </div>
      </div>
      {/* Progress bar */}
      <div className="h-1.5 rounded-full bg-surface-200 dark:bg-surface-700 overflow-hidden mb-2">
        <div
          className={cn(
            "h-full rounded-full transition-all",
            session.status === "failed" ? "bg-red-400" :
            session.status === "completed" ? "bg-blue-500" :
            session.status === "queued" ? "bg-yellow-400" : "bg-emerald-500"
          )}
          style={{ width: `${session.progress}%` }}
        />
      </div>
      <div className="flex items-center justify-between text-[11px] text-surface-800/50 dark:text-surface-400">
        <span>{session.progress}%</span>
        <div className="flex gap-3">
          {session.status === "running" && <span>{session.tokensPerSec} tok/s</span>}
          <span>{formatTokens(session.totalTokens)} tokens</span>
          {session.avgLatencyMs > 0 && <span>{session.avgLatencyMs}ms</span>}
        </div>
      </div>
    </button>
  );
}

// ── Pipeline Visualization (SVG) ──

function PipelineVisualization({ session }: { session: InferenceSession | null }) {
  const [animStep, setAnimStep] = useState(0);

  useEffect(() => {
    if (!session || session.status !== "running") return;
    const interval = setInterval(() => {
      setAnimStep(prev => (prev + 1) % 100);
    }, 200);
    return () => clearInterval(interval);
  }, [session]);

  if (!session) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-6 flex items-center justify-center h-64">
        <p className="text-sm text-surface-800/40 dark:text-surface-500">Select a session to view pipeline</p>
      </div>
    );
  }

  const stages = session.stages;
  const stageWidth = 140;
  const stageHeight = 80;
  const gap = 80;
  const totalWidth = stages.length * stageWidth + (stages.length - 1) * gap;
  const svgWidth = Math.max(totalWidth + 80, 600);
  const svgHeight = 200;
  const startY = 50;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-4 overflow-x-auto">
      <div className="flex items-center justify-between mb-3">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100">Pipeline: {session.modelName}</h3>
        <span className={cn(
          "text-[10px] font-medium uppercase tracking-wide px-2 py-0.5 rounded-full",
          session.strategy === "pipeline" ? "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300" :
          session.strategy === "tensor" ? "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300" :
          "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
        )}>
          {session.strategy} parallel
        </span>
      </div>
      <svg width={svgWidth} height={svgHeight} className="min-w-full">
        {/* Arrows between stages */}
        {stages.map((stage, i) => {
          if (i >= stages.length - 1) return null;
          const x1 = 40 + i * (stageWidth + gap) + stageWidth;
          const x2 = x1 + gap;
          const y1 = startY + stageHeight / 2;
          const y2 = y1;
          const midX = (x1 + x2) / 2;
          const isActive = stage.status === "active" || stage.status === "complete";
          return (
            <g key={`arrow-${i}`}>
              <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={isActive ? "#6366f1" : "#d1d5db"} strokeWidth="2" strokeDasharray={isActive ? "none" : "4 4"} />
              {isActive && (
                <circle cx={midX + (animStep % 40) - 20} cy={y2} r="3" fill="#6366f1" opacity="0.8">
                  <animate attributeName="cx" from={x1 + 5} to={x2 - 5} dur="1.5s" repeatCount="indefinite" />
                </circle>
              )}
              <polygon points={`${x2 - 6},${y2 - 4} ${x2},${y2} ${x2 - 6},${y2 + 4}`} fill={isActive ? "#6366f1" : "#d1d5db"} />
            </g>
          );
        })}
        {/* Stage boxes */}
        {stages.map((stage, i) => {
          const x = 40 + i * (stageWidth + gap);
          const y = startY;
          const color = providerColor(stage.provider);
          const isActive = stage.status === "active";
          return (
            <g key={`stage-${i}`}>
              <rect
                x={x} y={y} width={stageWidth} height={stageHeight} rx={8}
                fill={isActive ? `${color}15` : "#f9fafb"}
                stroke={isActive ? color : "#e5e7eb"}
                strokeWidth={isActive ? 2 : 1}
              />
              {isActive && (
                <rect x={x} y={y} width={stageWidth} height={stageHeight} rx={8} fill="none" stroke={color} strokeWidth="2" opacity="0.3">
                  <animate attributeName="opacity" values="0.3;0.1;0.3" dur="2s" repeatCount="indefinite" />
                </rect>
              )}
              {/* Stage label */}
              <text x={x + stageWidth / 2} y={y + 20} textAnchor="middle" fontSize="10" fontWeight="600" fill="#1f2937">{stage.label}</text>
              {/* Provider */}
              <text x={x + stageWidth / 2} y={y + 36} textAnchor="middle" fontSize="9" fill={color}>{stage.provider}</text>
              {/* Status dot */}
              <circle cx={x + 10} cy={y + 52} r="4" fill={
                stage.status === "active" ? "#10b981" :
                stage.status === "complete" ? "#3b82f6" :
                stage.status === "error" ? "#ef4444" : "#9ca3af"
              } />
              {/* Latency */}
              {stage.latencyMs > 0 && (
                <text x={x + 20} y={y + 55} fontSize="9" fill="#6b7280">{stage.latencyMs}ms</text>
              )}
              {/* Tokens/sec */}
              {stage.tokensPerSec > 0 && (
                <text x={x + stageWidth / 2} y={y + 68} textAnchor="middle" fontSize="9" fill="#6b7280">{stage.tokensPerSec} tok/s</text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ── CoT Routing Visualization ──

function CoTRoutingViz() {
  const [activeStep, setActiveStep] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setActiveStep(prev => (prev + 1) % COT_ROUTES.length);
    }, 2500);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100">Chain-of-Thought Routing</h3>
        <span className="text-[10px] text-surface-800/40 dark:text-surface-500 font-medium uppercase tracking-wide">Live</span>
      </div>
      <svg width="100%" height="260" viewBox="0 0 560 260" preserveAspectRatio="xMidYMid meet">
        {/* Connection lines */}
        {COT_ROUTES.map((route, i) => {
          if (i >= COT_ROUTES.length - 1) return null;
          const x1 = 60 + i * 85;
          const x2 = 60 + (i + 1) * 85;
          const y1 = 80;
          const y2 = 80;
          const isHighlighted = i === activeStep || i + 1 === activeStep;
          return (
            <g key={`cot-line-${i}`}>
              <line x1={x1 + 35} y1={y1} x2={x2} y2={y2} stroke={isHighlighted ? "#6366f1" : "#e5e7eb"} strokeWidth="2" />
              {isHighlighted && (
                <circle r="3" fill="#6366f1">
                  <animate attributeName="cx" from={x1 + 35} to={x2} dur="1s" repeatCount="indefinite" />
                  <animate attributeName="cy" from={y1} to={y2} dur="1s" repeatCount="indefinite" />
                </circle>
              )}
            </g>
          );
        })}
        {/* Nodes */}
        {COT_ROUTES.map((route, i) => {
          const cx = 60 + i * 85;
          const cy = 80;
          const isActive = i === activeStep;
          const color = providerColor(route.provider);
          return (
            <g key={`cot-node-${i}`}>
              <rect x={cx - 35} y={cy - 30} width={70} height={60} rx={8} fill={isActive ? `${color}20` : "#f9fafb"} stroke={isActive ? color : "#e5e7eb"} strokeWidth={isActive ? 2 : 1} />
              {isActive && (
                <rect x={cx - 35} y={cy - 30} width={70} height={60} rx={8} fill="none" stroke={color} strokeWidth="2" opacity="0.4">
                  <animate attributeName="opacity" values="0.4;0.1;0.4" dur="1.5s" repeatCount="indefinite" />
                </rect>
              )}
              <text x={cx} y={cy - 12} textAnchor="middle" fontSize="8" fontWeight="600" fill="#1f2937">{route.label}</text>
              <text x={cx} y={cy + 2} textAnchor="middle" fontSize="7" fill={color}>{route.provider}</text>
              <text x={cx} y={cy + 18} textAnchor="middle" fontSize="7" fill="#6b7280">{route.latencyMs}ms</text>
            </g>
          );
        })}
        {/* Legend */}
        <text x="60" y="145" fontSize="9" fontWeight="600" fill="#374151">Prompt Flow:</text>
        {COT_ROUTES.map((route, i) => {
          const x = 60 + i * 85;
          return (
            <g key={`cot-legend-${i}`}>
              <circle cx={x} cy={165} r="4" fill={providerColor(route.provider)} opacity={i <= activeStep ? 1 : 0.3} />
              <text x={x} y={180} textAnchor="middle" fontSize="7" fill={i <= activeStep ? "#374151" : "#9ca3af"}>{route.tokensProcessed} tok</text>
            </g>
          );
        })}
        {/* Active step info */}
        <rect x="60" y="200" width="440" height="45" rx={6} fill="#f3f4f6" />
        <text x="80" y="218" fontSize="9" fontWeight="600" fill="#374151">Active Step: {COT_ROUTES[activeStep].label}</text>
        <text x="80" y="233" fontSize="8" fill="#6b7280">Provider: {COT_ROUTES[activeStep].provider} | Latency: {COT_ROUTES[activeStep].latencyMs}ms | Tokens: {COT_ROUTES[activeStep].tokensProcessed}</text>
      </svg>
    </div>
  );
}

// ── Real-time Metrics ──

function RealtimeMetrics({ session }: { session: InferenceSession | null }) {
  const [metrics, setMetrics] = useState({ tokensPerSec: 0, totalTokens: 0, latencyPerStage: 0 });

  useEffect(() => {
    if (!session || session.status !== "running") {
      setMetrics({ tokensPerSec: 0, totalTokens: 0, latencyPerStage: 0 });
      return;
    }
    setMetrics({
      tokensPerSec: session.tokensPerSec,
      totalTokens: session.totalTokens,
      latencyPerStage: session.avgLatencyMs,
    });
    const interval = setInterval(() => {
      setMetrics(prev => ({
        tokensPerSec: Math.max(0, prev.tokensPerSec + Math.round((Math.random() - 0.5) * 12)),
        totalTokens: prev.totalTokens + Math.round(prev.tokensPerSec * 0.05),
        latencyPerStage: Math.max(10, prev.latencyPerStage + Math.round((Math.random() - 0.5) * 8)),
      }));
    }, 1500);
    return () => clearInterval(interval);
  }, [session]);

  if (!session) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-5">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100 mb-3">Real-time Metrics</h3>
        <p className="text-xs text-surface-800/40 dark:text-surface-500">Select a running session to see live metrics</p>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100">Real-time Metrics</h3>
        <span className="inline-flex items-center gap-1 text-[10px] text-emerald-600 dark:text-emerald-400 font-medium">
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
          LIVE
        </span>
      </div>
      <div className="grid grid-cols-3 gap-4 mb-4">
        <div className="text-center">
          <div className="text-2xl font-bold text-surface-900 dark:text-surface-100">{metrics.tokensPerSec}</div>
          <div className="text-[10px] text-surface-800/40 dark:text-surface-500 uppercase tracking-wide">Tokens/sec</div>
        </div>
        <div className="text-center">
          <div className="text-2xl font-bold text-surface-900 dark:text-surface-100">{formatTokens(metrics.totalTokens)}</div>
          <div className="text-[10px] text-surface-800/40 dark:text-surface-500 uppercase tracking-wide">Total Tokens</div>
        </div>
        <div className="text-center">
          <div className="text-2xl font-bold text-surface-900 dark:text-surface-100">{metrics.latencyPerStage}ms</div>
          <div className="text-[10px] text-surface-800/40 dark:text-surface-500 uppercase tracking-wide">Avg Stage Latency</div>
        </div>
      </div>
      {/* Stage latency breakdown */}
      <div className="border-t border-surface-100 dark:border-surface-800 pt-3">
        <div className="text-xs text-surface-800/40 dark:text-surface-500 font-medium mb-2">Latency per Stage</div>
        <div className="space-y-1.5">
          {session.stages.map(stage => (
            <div key={stage.stageIndex} className="flex items-center gap-2 text-xs">
              <span className="w-24 truncate text-surface-800/60 dark:text-surface-400">{stage.label}</span>
              <div className="flex-1 h-2 rounded-full bg-surface-100 dark:bg-surface-700 overflow-hidden">
                <div
                  className="h-full rounded-full transition-all"
                  style={{
                    width: `${session.stages.reduce((m, s) => Math.max(m, s.latencyMs), 1) > 0 ? (stage.latencyMs / session.stages.reduce((m, s) => Math.max(m, s.latencyMs), 1)) * 100 : 0}%`,
                    backgroundColor: providerColor(stage.provider),
                  }}
                />
              </div>
              <span className="w-12 text-right font-mono text-surface-800/60 dark:text-surface-400">{stage.latencyMs}ms</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Session Creation Form ──

function SessionCreationForm({ onClose }: { onClose: () => void }) {
  const [model, setModel] = useState(AVAILABLE_MODELS[0]);
  const [strategy, setStrategy] = useState<Strategy>("pipeline");
  const [selectedProviders, setSelectedProviders] = useState<string[]>(["NeuralForge"]);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const toggleProvider = useCallback((name: string) => {
    setSelectedProviders(prev =>
      prev.includes(name) ? prev.filter(p => p !== name) : [...prev, name]
    );
  }, []);

  const handleSubmit = useCallback(() => {
    setIsSubmitting(true);
    setTimeout(() => {
      setIsSubmitting(false);
      onClose();
    }, 1500);
  }, [onClose]);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100">New Inference Session</h3>
        <button onClick={onClose} className="text-surface-800/40 hover:text-surface-800/70 dark:text-surface-500 dark:hover:text-surface-300 text-lg transition-colors">&times;</button>
      </div>
      <div className="space-y-4">
        {/* Model selector */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 dark:text-surface-400 mb-1.5">Model</label>
          <select
            value={model}
            onChange={e => setModel(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-100 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
          >
            {AVAILABLE_MODELS.map(m => <option key={m} value={m}>{m}</option>)}
          </select>
        </div>
        {/* Strategy selector */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 dark:text-surface-400 mb-1.5">Strategy</label>
          <div className="flex gap-2">
            {(["pipeline", "tensor", "hybrid"] as Strategy[]).map(s => (
              <button
                key={s}
                onClick={() => setStrategy(s)}
                className={cn(
                  "flex-1 rounded-lg border px-3 py-2 text-xs font-medium capitalize transition-all",
                  strategy === s
                    ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/30 dark:text-brand-300"
                    : "border-surface-200 text-surface-800/60 hover:border-surface-300 dark:border-surface-700 dark:text-surface-400 dark:hover:border-surface-600"
                )}
              >
                {s}
              </button>
            ))}
          </div>
        </div>
        {/* Provider selection */}
        <div>
          <label className="block text-xs font-medium text-surface-800/60 dark:text-surface-400 mb-1.5">Preferred Providers</label>
          <div className="flex flex-wrap gap-2">
            {PROVIDERS.filter(p => p.status !== "offline").map(p => (
              <button
                key={p.id}
                onClick={() => toggleProvider(p.name)}
                className={cn(
                  "rounded-lg border px-3 py-1.5 text-xs font-medium transition-all flex items-center gap-1.5",
                  selectedProviders.includes(p.name)
                    ? cn(providerBorderClass(p.name), providerBgClass(p.name), providerTextClass(p.name))
                    : "border-surface-200 text-surface-800/40 hover:border-surface-300 dark:border-surface-700 dark:text-surface-500"
                )}
              >
                <span className="w-2 h-2 rounded-full" style={{ backgroundColor: p.color }} />
                {p.name}
              </button>
            ))}
          </div>
        </div>
        {/* Submit */}
        <button
          onClick={handleSubmit}
          disabled={selectedProviders.length === 0 || isSubmitting}
          className={cn(
            "w-full rounded-lg px-4 py-2.5 text-sm font-medium text-white transition-all",
            selectedProviders.length === 0 || isSubmitting
              ? "bg-surface-300 cursor-not-allowed dark:bg-surface-600"
              : "bg-brand-600 hover:bg-brand-700 active:bg-brand-800"
          )}
        >
          {isSubmitting ? "Starting Session..." : "Start Session"}
        </button>
      </div>
    </div>
  );
}

// ── Session History Table ──

type SortKey = "modelName" | "strategy" | "totalTokens" | "avgLatencyMs" | "tokensPerSec" | "createdAt";
type SortDir = "asc" | "desc";

function HistoryTable() {
  const [sortKey, setSortKey] = useState<SortKey>("createdAt");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const handleSort = useCallback((key: SortKey) => {
    if (sortKey === key) {
      setSortDir(d => d === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  }, [sortKey]);

  const sorted = useMemo(() => {
    return [...HISTORY].sort((a, b) => {
      const aVal = a[sortKey];
      const bVal = b[sortKey];
      const cmp = typeof aVal === "string" ? aVal.localeCompare(bVal as string) : (aVal as number) - (bVal as number);
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [sortKey, sortDir]);

  const SortIcon = ({ col }: { col: SortKey }) => (
    <span className="ml-1 text-[10px]">
      {sortKey === col ? (sortDir === "asc" ? "▲" : "▼") : "⇅"}
    </span>
  );

  const columns: { key: SortKey; label: string }[] = [
    { key: "modelName", label: "Model" },
    { key: "strategy", label: "Strategy" },
    { key: "totalTokens", label: "Tokens" },
    { key: "avgLatencyMs", label: "Latency" },
    { key: "tokensPerSec", label: "Tok/s" },
    { key: "createdAt", label: "Created" },
  ];

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 overflow-hidden">
      <div className="px-5 py-4 border-b border-surface-100 dark:border-surface-800">
        <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100">Session History</h3>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-surface-100 dark:border-surface-800">
              {columns.map(col => (
                <th
                  key={col.key}
                  className="px-5 py-3 text-left text-[10px] font-semibold uppercase tracking-wide text-surface-800/40 dark:text-surface-500 cursor-pointer hover:text-surface-800/70 dark:hover:text-surface-300 transition-colors"
                  onClick={() => handleSort(col.key)}
                >
                  {col.label} <SortIcon col={col.key} />
                </th>
              ))}
              <th className="px-5 py-3 text-left text-[10px] font-semibold uppercase tracking-wide text-surface-800/40 dark:text-surface-500">
                Status
              </th>
            </tr>
          </thead>
          <tbody>
            {sorted.map(entry => {
              const badge = sessionStatusBadge(entry.status);
              return (
                <tr key={entry.id} className="border-b border-surface-50 dark:border-surface-800/50 hover:bg-surface-50 dark:hover:bg-surface-800/30 transition-colors">
                  <td className="px-5 py-3 font-medium text-surface-900 dark:text-surface-100">{entry.modelName}</td>
                  <td className="px-5 py-3">
                    <span className={cn(
                      "rounded-full px-2 py-0.5 text-[10px] font-medium capitalize",
                      entry.strategy === "pipeline" ? "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300" :
                      entry.strategy === "tensor" ? "bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-300" :
                      "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
                    )}>
                      {entry.strategy}
                    </span>
                  </td>
                  <td className="px-5 py-3 font-mono text-surface-800/70 dark:text-surface-300">{formatTokens(entry.totalTokens)}</td>
                  <td className="px-5 py-3 font-mono text-surface-800/70 dark:text-surface-300">{entry.avgLatencyMs}ms</td>
                  <td className="px-5 py-3 font-mono text-surface-800/70 dark:text-surface-300">{entry.tokensPerSec > 0 ? entry.tokensPerSec : "-"}</td>
                  <td className="px-5 py-3 text-surface-800/50 dark:text-surface-400 text-xs">
                    {new Date(entry.createdAt).toLocaleDateString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}
                  </td>
                  <td className="px-5 py-3">
                    <span className={cn("rounded-full px-2 py-0.5 text-[10px] font-medium", badge.cls)}>{badge.text}</span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ── Aggregate Stats ──

function AggregateStats({ sessions }: { sessions: InferenceSession[] }) {
  const totalActive = sessions.filter(s => s.status === "running" || s.status === "queued").length;
  const totalTokens = sessions.reduce((sum, s) => sum + s.totalTokens, 0);
  const avgLatency = sessions.filter(s => s.avgLatencyMs > 0).reduce((sum, s, _, arr) => sum + s.avgLatencyMs / arr.filter(x => x.avgLatencyMs > 0).length, 0);

  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-4 text-center">
        <div className="text-2xl font-bold text-surface-900 dark:text-surface-100">{totalActive}</div>
        <div className="text-xs text-surface-800/40 dark:text-surface-500">Active Sessions</div>
      </div>
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-4 text-center">
        <div className="text-2xl font-bold text-brand-600 dark:text-brand-400">{formatTokens(totalTokens)}</div>
        <div className="text-xs text-surface-800/40 dark:text-surface-500">Tokens Processed</div>
      </div>
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-4 text-center">
        <div className="text-2xl font-bold text-surface-900 dark:text-surface-100">{Math.round(avgLatency)}ms</div>
        <div className="text-xs text-surface-800/40 dark:text-surface-500">Avg Latency</div>
      </div>
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-4 text-center">
        <div className="text-2xl font-bold text-emerald-600 dark:text-emerald-400">
          {PROVIDERS.filter(p => p.status === "online").length}/{PROVIDERS.length}
        </div>
        <div className="text-xs text-surface-800/40 dark:text-surface-500">Providers Online</div>
      </div>
    </div>
  );
}

// ── Main Page ──

export default function OrchestrationPage() {
  const [selectedSession, setSelectedSession] = useState<InferenceSession | null>(null);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [sessions, setSessions] = useState(ACTIVE_SESSIONS);
  const [activeTab, setActiveTab] = useState<"sessions" | "providers" | "history">("sessions");

  // Simulate live data updates
  useEffect(() => {
    const interval = setInterval(() => {
      setSessions(prev => prev.map(s => {
        if (s.status !== "running") return s;
        return {
          ...s,
          totalTokens: s.totalTokens + Math.round(s.tokensPerSec * 0.05),
          progress: Math.min(100, s.progress + Math.random() * 0.3),
          avgLatencyMs: Math.max(10, s.avgLatencyMs + Math.round((Math.random() - 0.5) * 6)),
          tokensPerSec: Math.max(10, s.tokensPerSec + Math.round((Math.random() - 0.5) * 8)),
          stages: s.stages.map(stage => ({
            ...stage,
            latencyMs: stage.status === "active" ? Math.max(5, stage.latencyMs + Math.round((Math.random() - 0.5) * 4)) : stage.latencyMs,
            tokensPerSec: stage.status === "active" ? Math.max(5, stage.tokensPerSec + Math.round((Math.random() - 0.5) * 6)) : stage.tokensPerSec,
          })),
        };
      }));
    }, 2000);
    return () => clearInterval(interval);
  }, []);

  // Auto-select first running session
  useEffect(() => {
    const firstRunning = sessions.find(s => s.status === "running");
    if (!selectedSession && firstRunning) {
      setSelectedSession(firstRunning);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-100">Cross-Provider Orchestration</h1>
          <p className="text-surface-800/60 dark:text-surface-400 text-sm mt-1">
            Manage distributed inference sessions across multiple GPU providers.
          </p>
        </div>
        <button
          onClick={() => setShowCreateForm(true)}
          className="rounded-lg bg-brand-600 hover:bg-brand-700 active:bg-brand-800 px-4 py-2 text-sm font-medium text-white transition-all shadow-sm hover:shadow-md"
        >
          + New Session
        </button>
      </div>

      {/* Aggregate Stats */}
      <div className="mb-6">
        <AggregateStats sessions={sessions} />
      </div>

      {/* Tab navigation */}
      <div className="flex gap-1 mb-6 border-b border-surface-100 dark:border-surface-800">
        {([
          { key: "sessions" as const, label: "Active Sessions" },
          { key: "providers" as const, label: "Provider Status" },
          { key: "history" as const, label: "Session History" },
        ]).map(tab => (
          <button
            key={tab.key}
            className={cn(
              "px-4 py-2.5 text-sm font-medium transition-colors border-b-2 -mb-px",
              activeTab === tab.key
                ? "border-brand-600 text-brand-600 dark:text-brand-400"
                : "border-transparent text-surface-800/50 hover:text-surface-800/70 dark:text-surface-400 dark:hover:text-surface-300"
            )}
            onClick={() => setActiveTab(tab.key)}
          >
            {tab.label}
            {tab.key === "sessions" && (
              <span className="ml-1.5 text-[10px] bg-brand-100 text-brand-700 dark:bg-brand-900/40 dark:text-brand-300 rounded-full px-1.5 py-0.5 font-medium">
                {sessions.filter(s => s.status === "running" || s.status === "queued").length}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Sessions Tab */}
      {activeTab === "sessions" && (
        <div>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Session list */}
            <div className="lg:col-span-1 space-y-3 max-h-[500px] overflow-y-auto pr-1">
              {sessions.map(session => (
                <SessionCard
                  key={session.id}
                  session={session}
                  isSelected={selectedSession?.id === session.id}
                  onClick={() => setSelectedSession(session)}
                />
              ))}
            </div>
            {/* Pipeline + Metrics */}
            <div className="lg:col-span-2 space-y-6">
              <PipelineVisualization session={selectedSession} />
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <RealtimeMetrics session={selectedSession} />
                {/* Stage details for selected session */}
                {selectedSession && (
                  <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 p-5">
                    <h3 className="font-semibold text-sm text-surface-900 dark:text-surface-100 mb-3">Stage Details</h3>
                    <div className="space-y-2">
                      {selectedSession.stages.map(stage => (
                        <div key={stage.stageIndex} className={cn(
                          "rounded-lg border p-3 transition-all",
                          stage.status === "active" ? providerBorderClass(stage.provider) : "border-surface-200 dark:border-surface-700",
                          providerBgClass(stage.provider)
                        )}>
                          <div className="flex items-center justify-between mb-1">
                            <div className="flex items-center gap-2">
                              {stageStatusIndicator(stage.status)}
                              <span className="text-xs font-medium text-surface-900 dark:text-surface-100">{stage.label}</span>
                            </div>
                            <span className={cn("text-[10px] font-medium", providerTextClass(stage.provider))}>{stage.provider}</span>
                          </div>
                          <div className="flex items-center gap-3 text-[11px] text-surface-800/50 dark:text-surface-400">
                            {stage.latencyMs > 0 && <span>{stage.latencyMs}ms</span>}
                            {stage.tokensPerSec > 0 && <span>{stage.tokensPerSec} tok/s</span>}
                            <span className="capitalize text-[10px]">{stage.status}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
              {/* CoT Routing */}
              <CoTRoutingViz />
            </div>
          </div>
        </div>
      )}

      {/* Providers Tab */}
      {activeTab === "providers" && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {PROVIDERS.map(provider => (
            <ProviderStatusCard key={provider.id} provider={provider} />
          ))}
        </div>
      )}

      {/* History Tab */}
      {activeTab === "history" && (
        <HistoryTable />
      )}

      {/* Create Session Modal */}
      {showCreateForm && (
        <div className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-4" onClick={() => setShowCreateForm(false)}>
          <div className="w-full max-w-md" onClick={e => e.stopPropagation()}>
            <SessionCreationForm onClose={() => setShowCreateForm(false)} />
          </div>
        </div>
      )}
    </div>
  );
}
