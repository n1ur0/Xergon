"use client";

import { useState } from "react";
import type { AiPointsData, AiPointsModelBreakdown } from "@/lib/api/provider";

interface ModelPerformanceChartProps {
  aiPoints: AiPointsData | null;
}

type ViewMode = "tokens" | "points" | "requests";

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toFixed(0);
}

function getValue(m: AiPointsModelBreakdown, mode: ViewMode): number {
  switch (mode) {
    case "tokens":
      return m.totalTokens;
    case "points":
      return m.points;
    case "requests":
      // Estimate requests from tokens (~500 tokens/request)
      return Math.round(m.totalTokens / 500);
  }
}

const VIEW_OPTIONS: { key: ViewMode; label: string }[] = [
  { key: "tokens", label: "Tokens" },
  { key: "points", label: "Points" },
  { key: "requests", label: "Requests" },
];

export function ModelPerformanceChart({ aiPoints }: ModelPerformanceChartProps) {
  const [view, setView] = useState<ViewMode>("tokens");

  if (!aiPoints || aiPoints.byModel.length === 0) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4 flex items-center gap-2">
          <span className="text-lg">&#x1F3AF;</span> Model Performance
        </h2>
        <div className="flex items-center justify-center py-8 text-surface-800/40 text-sm">
          No inference activity yet. Start handling requests to see model metrics.
        </div>
      </div>
    );
  }

  // Sort by selected value, descending
  const sorted = [...aiPoints.byModel]
    .map((m) => ({ ...m, value: getValue(m, view) }))
    .sort((a, b) => b.value - a.value);

  const maxVal = Math.max(...sorted.map((m) => m.value), 1);
  const totalVal = sorted.reduce((s, m) => s + m.value, 0);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <div className="flex items-center justify-between mb-5">
        <h2 className="font-semibold flex items-center gap-2">
          <span className="text-lg">&#x1F3AF;</span> Model Performance
        </h2>

        {/* Toggle */}
        <div className="flex rounded-lg border border-surface-200 overflow-hidden text-xs">
          {VIEW_OPTIONS.map((opt) => (
            <button
              key={opt.key}
              onClick={() => setView(opt.key)}
              className={`px-3 py-1.5 font-medium transition-colors ${
                view === opt.key
                  ? "bg-brand-600 text-white"
                  : "bg-surface-50 text-surface-800/60 hover:bg-surface-100"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      <div className="space-y-3">
        {sorted.map((m, i) => {
          const pct = totalVal > 0 ? (m.value / totalVal) * 100 : 0;
          const barPct = maxVal > 0 ? (m.value / maxVal) * 100 : 0;
          const isTop = i < 3;

          return (
            <div key={m.model}>
              <div className="flex items-center justify-between mb-1">
                <div className="flex items-center gap-2 min-w-0">
                  <span
                    className={`text-xs font-bold w-5 text-center ${
                      isTop ? "text-brand-600" : "text-surface-800/30"
                    }`}
                  >
                    {i + 1}
                  </span>
                  <span className="text-sm font-medium truncate">{m.model}</span>
                </div>
                <div className="flex items-center gap-2 shrink-0 ml-3">
                  <span
                    className={`text-xs ${
                      isTop ? "text-surface-800/50" : "text-surface-800/40"
                    }`}
                  >
                    {pct.toFixed(1)}%
                  </span>
                  <span
                    className={`text-sm font-mono font-medium min-w-[60px] text-right ${
                      isTop ? "text-brand-600" : "text-surface-800/70"
                    }`}
                  >
                    {formatNumber(m.value)}
                  </span>
                </div>
              </div>
              {/* Bar */}
              <div className="flex items-center gap-2">
                <div className="flex-1 h-2.5 rounded-full bg-surface-100 overflow-hidden">
                  <div
                    className={`h-full rounded-full transition-all duration-500 ${
                      isTop
                        ? "bg-gradient-to-r from-brand-500 to-brand-400"
                        : "bg-surface-300"
                    }`}
                    style={{ width: `${Math.max(barPct, 1)}%` }}
                  />
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Summary footer */}
      <div className="mt-5 pt-4 border-t border-surface-100 flex items-center justify-between text-xs text-surface-800/50">
        <span>{sorted.length} model{sorted.length !== 1 ? "s" : ""}</span>
        <span>
          Total:{" "}
          <span className="font-mono font-medium text-surface-800/70">
            {formatNumber(totalVal)}{" "}
            {view === "tokens" ? "tokens" : view === "points" ? "pts" : "reqs"}
          </span>
        </span>
      </div>
    </div>
  );
}
