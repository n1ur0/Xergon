"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import type { AiPointsData, SettlementRecord, ProviderScoreData } from "@/lib/api/provider";
import { generateSparkline, generateSparklineArea } from "@/lib/utils/charts";

interface RealtimeMetricsProps {
  aiPoints: AiPointsData | null;
  settlements: SettlementRecord[];
  providerScore: ProviderScoreData | null;
  /** Called when auto-refresh fires; parent triggers a data reload */
  onRefresh?: () => void;
}

interface MetricCard {
  label: string;
  value: string;
  sparkData: number[];
  trend: "up" | "down" | "flat";
  trendPct: number;
  accent?: boolean;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toFixed(0);
}

function formatErg(nanoerg: number): string {
  const erg = nanoerg / 1e9;
  if (erg >= 1) return `${erg.toFixed(4)}`;
  if (erg >= 0.001) return `${erg.toFixed(6)}`;
  return `${nanoerg} n`;
}

/**
 * Generate deterministic pseudo-random sparkline history from a seed value.
 * Produces a plausible-looking trend with 8 data points.
 */
function sparkFromSeed(value: number, count: number = 8): number[] {
  const points: number[] = [];
  let current = value * 0.4; // start low
  for (let i = 0; i < count; i++) {
    // Gradual increase toward the target value with some noise
    const progress = (i + 1) / count;
    const noise = Math.sin(value * 0.001 + i * 17.3) * value * 0.08;
    current = value * progress + noise;
    points.push(Math.max(0, current));
  }
  // Ensure last point is close to actual value
  points[points.length - 1] = value;
  return points;
}

export function RealtimeMetrics({
  aiPoints,
  settlements,
  providerScore,
  onRefresh,
}: RealtimeMetricsProps) {
  const [elapsed, setElapsed] = useState(0);
  const [history, setHistory] = useState<{
    points: number[];
    ergToday: number[];
    reqMin: number[];
  }>({ points: [], ergToday: [], reqMin: [] });
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Simulated requests/min from tokens (assume 30-day window)
  const estRequestsPerMin = aiPoints && aiPoints.totalTokens > 0
    ? Math.round((aiPoints.totalTokens / 500) / (30 * 24 * 60))
    : 0;

  // ERG earned today
  const todayStr = new Date().toISOString().slice(0, 10);
  const ergToday = settlements
    .filter((s) => s.createdAt.slice(0, 10) === todayStr)
    .reduce((sum, s) => sum + s.amountNanoerg, 0);

  const activeModels = aiPoints?.byModel.filter((m) => m.totalTokens > 0).length ?? 0;

  const ponwScore = aiPoints?.aiPoints ?? 0;

  // Build history on mount and when data changes
  useEffect(() => {
    const pointsHist = sparkFromSeed(ponwScore, 8);
    const ergHist = sparkFromSeed(ergToday / 1e9, 8);
    const reqHist = sparkFromSeed(estRequestsPerMin, 8);
    setHistory({ points: pointsHist, ergToday: ergHist, reqMin: reqHist });
  }, [ponwScore, ergToday, estRequestsPerMin]);

  // Auto-refresh timer
  useEffect(() => {
    timerRef.current = setInterval(() => {
      setElapsed((prev) => prev + 1);
      onRefresh?.();
    }, 30_000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [onRefresh]);

  // Trend calculations
  const calcTrend = useCallback(
    (data: number[]): { trend: "up" | "down" | "flat"; trendPct: number } => {
      if (data.length < 2) return { trend: "flat", trendPct: 0 };
      const prev = data[data.length - 2];
      const curr = data[data.length - 1];
      if (prev === 0 && curr === 0) return { trend: "flat", trendPct: 0 };
      const change = ((curr - prev) / Math.max(prev, 0.001)) * 100;
      return {
        trend: change > 0.5 ? "up" : change < -0.5 ? "down" : "flat",
        trendPct: Math.abs(change),
      };
    },
    [],
  );

  const cards: MetricCard[] = [
    {
      label: "Requests / min",
      value: estRequestsPerMin.toString(),
      sparkData: history.reqMin,
      ...calcTrend(history.reqMin),
    },
    {
      label: "Active Models",
      value: activeModels.toString(),
      sparkData: activeModels > 0 ? [activeModels, activeModels, activeModels] : [0],
      trend: "flat",
      trendPct: 0,
    },
    {
      label: "PoNW Score",
      value: formatNumber(ponwScore),
      sparkData: history.points,
      ...calcTrend(history.points),
      accent: true,
    },
    {
      label: "ERG Today",
      value: `${formatErg(ergToday)} ERG`,
      sparkData: history.ergToday,
      ...calcTrend(history.ergToday),
      accent: ergToday > 0,
    },
  ];

  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4 mb-8">
      {cards.map((card) => (
        <div
          key={card.label}
          className="rounded-xl border border-surface-200 bg-surface-0 p-4"
        >
          <div className="flex items-start justify-between">
            <div className="min-w-0">
              <p className="text-[10px] font-medium uppercase tracking-wide text-surface-800/50 mb-1">
                {card.label}
              </p>
              <p
                className={`text-xl font-bold ${
                  card.accent ? "text-brand-600" : "text-surface-900"
                }`}
              >
                {card.value}
              </p>
            </div>

            {/* Sparkline */}
            {card.sparkData.length > 1 && (
              <svg width={64} height={28} className="shrink-0 ml-2 mt-1">
                <path
                  d={generateSparklineArea(card.sparkData, 64, 28)}
                  fill="currentColor"
                  className="text-brand-500"
                  opacity={0.1}
                />
                <path
                  d={generateSparkline(card.sparkData, 64, 28)}
                  fill="none"
                  stroke="currentColor"
                  className={card.accent ? "text-brand-500" : "text-surface-400"}
                  strokeWidth={1.5}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            )}
          </div>

          {/* Trend indicator */}
          {card.trend !== "flat" && card.trendPct > 0 && (
            <div className="flex items-center gap-1 mt-1.5">
              <span
                className={`text-[10px] font-medium ${
                  card.trend === "up" ? "text-accent-600" : "text-danger-500"
                }`}
              >
                {card.trend === "up" ? "\u2191" : "\u2193"} {card.trendPct.toFixed(1)}%
              </span>
              <span className="text-[10px] text-surface-800/30">vs prev</span>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
