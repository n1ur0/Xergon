"use client";

import { useState, useEffect, useCallback } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface ShardVisualization {
  shardIndex: number;
  provider: string;
  model: string;
  layerRange: string;
  status: "connected" | "syncing" | "error";
  latencyMs: number;
  utilization: number;
}

interface ShardingView {
  type: "pipeline" | "tensor";
  model: string;
  totalShards: number;
  shards: ShardVisualization[];
  totalLatency: number;
}

// ── Mock Data ──

const PIPELINE_SHARDS: ShardVisualization[] = [
  { shardIndex: 0, provider: "NeuralForge", model: "Llama-3.1-70B", layerRange: "0-11", status: "connected", latencyMs: 45, utilization: 82 },
  { shardIndex: 1, provider: "GPUHive", model: "Llama-3.1-70B", layerRange: "12-23", status: "connected", latencyMs: 52, utilization: 75 },
  { shardIndex: 2, provider: "TensorNode", model: "Llama-3.1-70B", layerRange: "24-35", status: "syncing", latencyMs: 68, utilization: 60 },
  { shardIndex: 3, provider: "DeepCompute", model: "Llama-3.1-70B", layerRange: "36-47", status: "connected", latencyMs: 41, utilization: 88 },
  { shardIndex: 4, provider: "InferX", model: "Llama-3.1-70B", layerRange: "48-59", status: "connected", latencyMs: 55, utilization: 70 },
  { shardIndex: 5, provider: "NeuralForge", model: "Llama-3.1-70B", layerRange: "60-71", status: "error", latencyMs: 0, utilization: 0 },
  { shardIndex: 6, provider: "GPUHive", model: "Llama-3.1-70B", layerRange: "72-79", status: "connected", latencyMs: 38, utilization: 92 },
];

const TENSOR_SHARDS: ShardVisualization[] = [
  { shardIndex: 0, provider: "NeuralForge", model: "Llama-3.1-70B", layerRange: "Attn heads 0-15", status: "connected", latencyMs: 22, utilization: 78 },
  { shardIndex: 1, provider: "GPUHive", model: "Llama-3.1-70B", layerRange: "Attn heads 16-31", status: "connected", latencyMs: 25, utilization: 71 },
  { shardIndex: 2, provider: "TensorNode", model: "Llama-3.1-70B", layerRange: "Attn heads 32-47", status: "connected", latencyMs: 28, utilization: 85 },
  { shardIndex: 3, provider: "DeepCompute", model: "Llama-3.1-70B", layerRange: "Attn heads 48-63", status: "syncing", latencyMs: 35, utilization: 45 },
];

const PROVIDER_COLORS: Record<string, string> = {
  NeuralForge: "bg-blue-500",
  GPUHive: "bg-emerald-500",
  TensorNode: "bg-purple-500",
  DeepCompute: "bg-orange-500",
  InferX: "bg-pink-500",
};

const PROVIDER_BORDER: Record<string, string> = {
  NeuralForge: "border-blue-500",
  GPUHive: "border-emerald-500",
  TensorNode: "border-purple-500",
  DeepCompute: "border-orange-500",
  InferX: "border-pink-500",
};

const PROVIDER_BG: Record<string, string> = {
  NeuralForge: "bg-blue-500/10",
  GPUHive: "bg-emerald-500/10",
  TensorNode: "bg-purple-500/10",
  DeepCompute: "bg-orange-500/10",
  InferX: "bg-pink-500/10",
};

// ── Helpers ──

function statusIndicator(status: ShardVisualization["status"]) {
  switch (status) {
    case "connected":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse" title="Connected" />;
    case "syncing":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-yellow-500 animate-spin" title="Syncing" />;
    case "error":
      return <span className="inline-block w-2.5 h-2.5 rounded-full bg-red-500" title="Error" />;
  }
}

function statusLabel(status: ShardVisualization["status"]): { text: string; cls: string } {
  switch (status) {
    case "connected":
      return { text: "Connected", cls: "text-emerald-700 bg-emerald-100" };
    case "syncing":
      return { text: "Syncing", cls: "text-yellow-700 bg-yellow-100" };
    case "error":
      return { text: "Error", cls: "text-red-700 bg-red-100" };
  }
}

// ── Pipeline Parallel View ──

function PipelineView({ shards, onShardClick, selectedShard }: {
  shards: ShardVisualization[];
  onShardClick: (shard: ShardVisualization) => void;
  selectedShard: number | null;
}) {
  return (
    <div className="space-y-3">
      {shards.map((shard, i) => (
        <div key={shard.shardIndex} className="flex items-center gap-3">
          {/* Shard block */}
          <button
            className={cn(
              "flex-1 rounded-xl border-2 p-4 text-left transition-all hover:shadow-md",
              PROVIDER_BORDER[shard.provider] ?? "border-gray-300",
              PROVIDER_BG[shard.provider] ?? "bg-gray-50",
              selectedShard === shard.shardIndex && "ring-2 ring-brand-500 ring-offset-2",
              shard.status === "error" && "opacity-60 border-red-300 bg-red-50"
            )}
            onClick={() => onShardClick(shard)}
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                {statusIndicator(shard.status)}
                <span className="font-semibold text-surface-900 text-sm">
                  Shard {shard.shardIndex}
                </span>
              </div>
              <span className="text-xs text-surface-800/40 font-mono">
                Layers {shard.layerRange}
              </span>
            </div>

            <div className="flex items-center justify-between text-sm">
              <span className={cn(
                "rounded-full px-2 py-0.5 text-xs font-medium",
                PROVIDER_BG[shard.provider],
                "text-surface-800/70"
              )}>
                {shard.provider}
              </span>
              <div className="flex items-center gap-3 text-xs text-surface-800/50">
                <span>{shard.latencyMs}ms</span>
                <span>{shard.utilization}%</span>
              </div>
            </div>

            {/* Utilization bar */}
            <div className="mt-2 h-1.5 rounded-full bg-surface-200 overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all",
                  shard.utilization > 80 ? "bg-emerald-500" :
                  shard.utilization > 50 ? "bg-yellow-500" : "bg-red-400"
                )}
                style={{ width: `${shard.utilization}%` }}
              />
            </div>
          </button>

          {/* Arrow */}
          {i < shards.length - 1 && (
            <div className="flex flex-col items-center gap-1 flex-shrink-0">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" className="text-surface-300">
                <path d="M12 4L12 18M12 18L6 12M12 18L18 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
              <span className="text-[10px] text-surface-800/30">data</span>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

// ── Tensor Parallel View ──

function TensorView({ shards, onShardClick, selectedShard }: {
  shards: ShardVisualization[];
  onShardClick: (shard: ShardVisualization) => void;
  selectedShard: number | null;
}) {
  return (
    <div>
      {/* Single layer visualization */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
        <div className="text-xs text-surface-800/40 mb-3 font-medium">Layer 0 — Attention Heads Split</div>
        <div className="flex items-stretch gap-2">
          {shards.map((shard, i) => (
            <div key={shard.shardIndex} className="flex items-center gap-2 flex-1">
              <button
                className={cn(
                  "flex-1 rounded-lg border-2 p-3 text-center transition-all hover:shadow-md min-h-[100px] flex flex-col justify-center",
                  PROVIDER_BORDER[shard.provider] ?? "border-gray-300",
                  PROVIDER_BG[shard.provider] ?? "bg-gray-50",
                  selectedShard === shard.shardIndex && "ring-2 ring-brand-500 ring-offset-2",
                  shard.status === "error" && "opacity-60 border-red-300 bg-red-50"
                )}
                onClick={() => onShardClick(shard)}
              >
                <div className="flex items-center justify-center gap-1 mb-1">
                  {statusIndicator(shard.status)}
                  <span className="text-xs font-semibold text-surface-900">
                    GPU {shard.shardIndex}
                  </span>
                </div>
                <div className="text-[10px] text-surface-800/40 mb-1">
                  {shard.layerRange}
                </div>
                <div className="text-xs text-surface-800/50">{shard.provider}</div>
                <div className="mt-1 h-1 rounded-full bg-surface-200 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-brand-500 transition-all"
                    style={{ width: `${shard.utilization}%` }}
                  />
                </div>
              </button>

              {/* All-to-all arrows (bidirectional for tensor parallel) */}
              {i < shards.length - 1 && (
                <div className="flex flex-col items-center justify-center flex-shrink-0">
                  <svg width="20" height="20" viewBox="0 0 20 20" fill="none" className="text-surface-300">
                    <path d="M10 2V16M10 16L5 11M10 16L15 11" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                  <svg width="20" height="20" viewBox="0 0 20 20" fill="none" className="text-surface-300 -mt-1">
                    <path d="M10 16V2M10 2L5 7M10 2L15 7" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                  <span className="text-[8px] text-surface-800/30 mt-0.5">all2all</span>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Shard details grid */}
      <div className="grid grid-cols-2 gap-3">
        {shards.map((shard) => (
          <button
            key={shard.shardIndex}
            className={cn(
              "rounded-xl border p-3 text-left transition-all hover:shadow-sm",
              selectedShard === shard.shardIndex
                ? "border-brand-500 bg-brand-500/5"
                : "border-surface-200 bg-surface-0"
            )}
            onClick={() => onShardClick(shard)}
          >
            <div className="flex items-center justify-between mb-1">
              <div className="flex items-center gap-1.5">
                {statusIndicator(shard.status)}
                <span className="text-sm font-medium text-surface-900">Shard {shard.shardIndex}</span>
              </div>
              <span className="text-xs text-surface-800/40">{shard.latencyMs}ms</span>
            </div>
            <div className="text-xs text-surface-800/50">{shard.provider}</div>
            <div className="mt-1.5 h-1 rounded-full bg-surface-200 overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all",
                  shard.utilization > 80 ? "bg-emerald-500" :
                  shard.utilization > 50 ? "bg-yellow-500" : "bg-red-400"
                )}
                style={{ width: `${shard.utilization}%` }}
              />
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Shard Detail Panel ──

function ShardDetailPanel({ shard, onClose }: { shard: ShardVisualization | null; onClose: () => void }) {
  if (!shard) return null;

  const badge = statusLabel(shard.status);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 animate-in fade-in slide-in-from-right-2">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold text-surface-900">Shard {shard.shardIndex} Details</h3>
        <button
          className="text-surface-800/40 hover:text-surface-800/70 transition-colors text-lg"
          onClick={onClose}
        >
          &times;
        </button>
      </div>

      <div className="space-y-3">
        <div className="flex justify-between items-center">
          <span className="text-sm text-surface-800/50">Status</span>
          <span className={cn("rounded-full px-2.5 py-0.5 text-xs font-medium", badge.cls)}>
            {badge.text}
          </span>
        </div>
        <div className="flex justify-between">
          <span className="text-sm text-surface-800/50">Provider</span>
          <span className="text-sm font-medium text-surface-900">{shard.provider}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-sm text-surface-800/50">Model</span>
          <span className="text-sm font-medium text-surface-900">{shard.model}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-sm text-surface-800/50">Layer Range</span>
          <span className="text-sm font-mono text-surface-900">{shard.layerRange}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-sm text-surface-800/50">Latency</span>
          <span className="text-sm font-mono text-surface-900">
            {shard.latencyMs > 0 ? `${shard.latencyMs}ms` : "N/A"}
          </span>
        </div>

        {/* Utilization */}
        <div>
          <div className="flex justify-between mb-1">
            <span className="text-sm text-surface-800/50">GPU Utilization</span>
            <span className="text-sm font-medium text-surface-900">{shard.utilization}%</span>
          </div>
          <div className="h-2.5 rounded-full bg-surface-200 overflow-hidden">
            <div
              className={cn(
                "h-full rounded-full transition-all",
                shard.utilization > 80 ? "bg-emerald-500" :
                shard.utilization > 50 ? "bg-yellow-500" : "bg-red-400"
              )}
              style={{ width: `${shard.utilization}%` }}
            />
          </div>
        </div>

        {/* Latency breakdown */}
        {shard.status !== "error" && (
          <div className="mt-4 pt-3 border-t border-surface-100">
            <div className="text-xs text-surface-800/40 mb-2 font-medium">Latency Breakdown</div>
            <div className="space-y-1.5">
              {[
                { label: "Compute", value: Math.round(shard.latencyMs * 0.5), color: "bg-blue-400" },
                { label: "Network I/O", value: Math.round(shard.latencyMs * 0.25), color: "bg-purple-400" },
                { label: "Memory Transfer", value: Math.round(shard.latencyMs * 0.15), color: "bg-orange-400" },
                { label: "Queue Wait", value: Math.round(shard.latencyMs * 0.1), color: "bg-pink-400" },
              ].map((item) => (
                <div key={item.label} className="flex items-center gap-2 text-xs">
                  <span className="w-20 text-surface-800/50">{item.label}</span>
                  <div className="flex-1 h-1.5 rounded-full bg-surface-100 overflow-hidden">
                    <div
                      className={cn("h-full rounded-full", item.color)}
                      style={{ width: `${(item.value / shard.latencyMs) * 100}%` }}
                    />
                  </div>
                  <span className="w-10 text-right font-mono text-surface-800/60">{item.value}ms</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Provider GPU info */}
        <div className="mt-4 pt-3 border-t border-surface-100">
          <div className="text-xs text-surface-800/40 mb-2 font-medium">Provider GPU</div>
          <div className="grid grid-cols-2 gap-2 text-xs">
            <div className="rounded-lg bg-surface-50 p-2">
              <div className="text-surface-800/40">GPU</div>
              <div className="font-medium text-surface-900">A100 80GB</div>
            </div>
            <div className="rounded-lg bg-surface-50 p-2">
              <div className="text-surface-800/40">VRAM Used</div>
              <div className="font-medium text-surface-900">{Math.round(shard.utilization * 0.72)} GB</div>
            </div>
            <div className="rounded-lg bg-surface-50 p-2">
              <div className="text-surface-800/40">Temperature</div>
              <div className="font-medium text-surface-900">{62 + Math.round(shard.utilization * 0.1)}&deg;C</div>
            </div>
            <div className="rounded-lg bg-surface-50 p-2">
              <div className="text-surface-800/40">Power Draw</div>
              <div className="font-medium text-surface-900">{Math.round(150 + shard.utilization * 2.5)}W</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Latency Breakdown Bar ──

function LatencyBreakdownBar({ shards }: { shards: ShardVisualization[] }) {
  const activeShards = shards.filter(s => s.status !== "error");
  if (activeShards.length === 0) return null;

  const totalLatency = activeShards.reduce((sum, s) => sum + s.latencyMs, 0);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="font-semibold text-surface-900 mb-1">Latency Breakdown</h3>
      <p className="text-xs text-surface-800/40 mb-3">Total: {totalLatency}ms across {activeShards.length} shards</p>

      {/* Stacked bar */}
      <div className="h-6 rounded-full overflow-hidden flex mb-3">
        {activeShards.map((shard) => {
          const width = totalLatency > 0 ? (shard.latencyMs / totalLatency) * 100 : 0;
          return (
            <div
              key={shard.shardIndex}
              className={cn(
                "h-full transition-all",
                PROVIDER_COLORS[shard.provider] ?? "bg-gray-400",
                shard.shardIndex > 0 && "border-l border-white"
              )}
              style={{ width: `${width}%` }}
              title={`Shard ${shard.shardIndex}: ${shard.latencyMs}ms`}
            />
          );
        })}
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-3">
        {activeShards.map((shard) => (
          <div key={shard.shardIndex} className="flex items-center gap-1.5 text-xs">
            <span className={cn("w-2.5 h-2.5 rounded-sm", PROVIDER_COLORS[shard.provider] ?? "bg-gray-400")} />
            <span className="text-surface-800/50">S{shard.shardIndex}</span>
            <span className="font-mono text-surface-800/70">{shard.latencyMs}ms</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Data Flow Animation ──

function DataFlowAnimation({ shards, view }: { shards: ShardVisualization[]; view: "pipeline" | "tensor" }) {
  const [step, setStep] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setStep((prev) => (prev + 1) % (shards.length + 2));
    }, 1200);
    return () => clearInterval(interval);
  }, [shards.length]);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="font-semibold text-surface-900 mb-3">Data Flow</h3>
      <div className="flex items-center gap-2 overflow-x-auto pb-2">
        {shards.map((shard, i) => {
          const isActive = view === "pipeline"
            ? step === i
            : step >= i && step < i + 2;

          return (
            <div key={shard.shardIndex} className="flex items-center gap-2 flex-shrink-0">
              <div className={cn(
                "w-14 h-14 rounded-lg border-2 flex flex-col items-center justify-center transition-all",
                PROVIDER_BORDER[shard.provider] ?? "border-gray-300",
                PROVIDER_BG[shard.provider] ?? "bg-gray-50",
                isActive ? "scale-110 shadow-md" : "opacity-40"
              )}>
                <span className="text-xs font-bold text-surface-900">S{i}</span>
                <span className="text-[8px] text-surface-800/40">{shard.provider.slice(0, 6)}</span>
              </div>

              {i < shards.length - 1 && (
                <div className="flex-shrink-0">
                  <svg width="32" height="20" viewBox="0 0 32 20" className={cn("transition-opacity", isActive ? "opacity-100" : "opacity-20")}>
                    <circle cx="4" cy="10" r="2" fill="#6366f1" opacity={step % 2 === 0 ? 1 : 0.3} />
                    <circle cx="12" cy="10" r="2" fill="#6366f1" opacity={step % 2 === 1 ? 1 : 0.3} />
                    <circle cx="20" cy="10" r="2" fill="#6366f1" opacity={step % 2 === 0 ? 1 : 0.3} />
                    <circle cx="28" cy="10" r="2" fill="#6366f1" opacity={step % 2 === 1 ? 1 : 0.3} />
                  </svg>
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="text-xs text-surface-800/30 mt-2 text-center">
        {view === "pipeline" ? "Sequential layer processing" : "Parallel attention head processing"}
      </div>
    </div>
  );
}

// ── Provider Utilization Bars ──

function ProviderUtilizationBars({ shards }: { shards: ShardVisualization[] }) {
  const providerMap = new Map<string, { shards: number; totalUtil: number; totalLatency: number }>();
  for (const s of shards) {
    const entry = providerMap.get(s.provider) ?? { shards: 0, totalUtil: 0, totalLatency: 0 };
    entry.shards += 1;
    entry.totalUtil += s.utilization;
    entry.totalLatency += s.latencyMs;
    providerMap.set(s.provider, entry);
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="font-semibold text-surface-900 mb-3">Provider GPU Utilization</h3>
      <div className="space-y-3">
        {Array.from(providerMap.entries()).sort((a, b) => b[1].shards - a[1].shards).map(([provider, data]) => {
          const avgUtil = Math.round(data.totalUtil / data.shards);
          const avgLatency = Math.round(data.totalLatency / data.shards);
          return (
            <div key={provider} className="flex items-center gap-3">
              <div className={cn("w-3 h-3 rounded-sm flex-shrink-0", PROVIDER_COLORS[provider] ?? "bg-gray-400")} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between mb-0.5">
                  <span className="text-sm font-medium text-surface-900 truncate">{provider}</span>
                  <span className="text-xs text-surface-800/40">{data.shards} shard{data.shards !== 1 ? "s" : ""} · {avgLatency}ms avg</span>
                </div>
                <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
                  <div
                    className={cn(
                      "h-full rounded-full transition-all",
                      PROVIDER_COLORS[provider] ?? "bg-gray-400"
                    )}
                    style={{ width: `${avgUtil}%` }}
                  />
                </div>
              </div>
              <span className="text-sm font-mono text-surface-800/60 w-10 text-right">{avgUtil}%</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Main Page ──

export default function ShardingPage() {
  const [viewType, setViewType] = useState<"pipeline" | "tensor">("pipeline");
  const [selectedShard, setSelectedShard] = useState<ShardVisualization | null>(null);

  const currentShards = viewType === "pipeline" ? PIPELINE_SHARDS : TENSOR_SHARDS;
  const totalLatency = currentShards
    .filter(s => s.status !== "error")
    .reduce((sum, s) => sum + s.latencyMs, 0);

  const handleShardClick = useCallback((shard: ShardVisualization) => {
    setSelectedShard(prev => prev?.shardIndex === shard.shardIndex ? null : shard);
  }, []);

  // Simulate latency fluctuations
  const [shards, setShards] = useState(currentShards);
  useEffect(() => {
    const interval = setInterval(() => {
      setShards(prev => prev.map(s => ({
        ...s,
        latencyMs: s.status === "connected"
          ? s.latencyMs + Math.round((Math.random() - 0.5) * 6)
          : s.latencyMs,
        utilization: s.status === "connected"
          ? Math.max(20, Math.min(100, s.utilization + Math.round((Math.random() - 0.5) * 8)))
          : s.utilization,
      })));
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  // Reset shards when view changes
  useEffect(() => {
    setShards(currentShards);
    setSelectedShard(null);
  }, [viewType]);

  const connectedCount = shards.filter(s => s.status === "connected").length;
  const errorCount = shards.filter(s => s.status === "error").length;
  const syncingCount = shards.filter(s => s.status === "syncing").length;

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Model Sharding Visualizer</h1>
      <p className="text-surface-800/60 mb-6">
        See how model layers and attention heads are distributed across GPU providers.
      </p>

      {/* View toggle */}
      <div className="flex gap-1 mb-6 border-b border-surface-100">
        {(["pipeline", "tensor"] as const).map((t) => (
          <button
            key={t}
            className={cn(
              "px-4 py-2.5 text-sm font-medium capitalize transition-colors border-b-2 -mb-px",
              viewType === t
                ? "border-brand-600 text-brand-600"
                : "border-transparent text-surface-800/50 hover:text-surface-800/70"
            )}
            onClick={() => setViewType(t)}
          >
            {t === "pipeline" ? "Pipeline Parallel" : "Tensor Parallel"}
          </button>
        ))}
      </div>

      {/* Summary stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 text-center">
          <div className="text-2xl font-bold text-surface-900">{shards.length}</div>
          <div className="text-xs text-surface-800/40">Total Shards</div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 text-center">
          <div className="text-2xl font-bold text-emerald-600">{connectedCount}</div>
          <div className="text-xs text-surface-800/40">Connected</div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 text-center">
          <div className="text-2xl font-bold text-brand-600">{totalLatency}ms</div>
          <div className="text-xs text-surface-800/40">Total Latency</div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 text-center">
          <div className="text-2xl font-bold text-surface-900">
            {errorCount > 0 ? (
              <span className="text-red-600">{errorCount} err{syncingCount > 0 ? ` · ${syncingCount} sync` : ""}</span>
            ) : syncingCount > 0 ? (
              <span className="text-yellow-600">{syncingCount} syncing</span>
            ) : (
              <span className="text-emerald-600">Healthy</span>
            )}
          </div>
          <div className="text-xs text-surface-800/40">Health</div>
        </div>
      </div>

      {/* Main layout */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Shard view */}
        <div className="lg:col-span-2">
          {viewType === "pipeline" ? (
            <PipelineView shards={shards} onShardClick={handleShardClick} selectedShard={selectedShard?.shardIndex ?? null} />
          ) : (
            <TensorView shards={shards} onShardClick={handleShardClick} selectedShard={selectedShard?.shardIndex ?? null} />
          )}
        </div>

        {/* Detail panel */}
        <div className="space-y-6">
          <ShardDetailPanel shard={selectedShard} onClose={() => setSelectedShard(null)} />
          <DataFlowAnimation shards={shards} view={viewType} />
        </div>
      </div>

      {/* Bottom panels */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-6">
        <LatencyBreakdownBar shards={shards} />
        <ProviderUtilizationBars shards={shards} />
      </div>
    </div>
  );
}
