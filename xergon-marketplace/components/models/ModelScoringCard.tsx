"use client";

import { useMemo } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ScoreDimension {
  label: string;
  value: number; // 0-100
  weight: number; // 0-1
  description?: string;
}

export interface ModelScoreData {
  modelName: string;
  tier: string;
  dimensions: ScoreDimension[];
  category: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CATEGORY_AVERAGES: Record<string, ScoreDimension[]> = {
  "Large Language": [
    { label: "Speed", value: 62, weight: 0.25 },
    { label: "Quality", value: 71, weight: 0.3 },
    { label: "Cost", value: 58, weight: 0.2 },
    { label: "Reliability", value: 75, weight: 0.15 },
    { label: "Availability", value: 80, weight: 0.1 },
  ],
  "Code Models": [
    { label: "Speed", value: 68, weight: 0.25 },
    { label: "Quality", value: 65, weight: 0.3 },
    { label: "Cost", value: 70, weight: 0.2 },
    { label: "Reliability", value: 72, weight: 0.15 },
    { label: "Availability", value: 78, weight: 0.1 },
  ],
  "Small Models": [
    { label: "Speed", value: 85, weight: 0.25 },
    { label: "Quality", value: 55, weight: 0.3 },
    { label: "Cost", value: 90, weight: 0.2 },
    { label: "Reliability", value: 82, weight: 0.15 },
    { label: "Availability", value: 88, weight: 0.1 },
  ],
};

const DEFAULT_CATEGORY_AVG: ScoreDimension[] = [
  { label: "Speed", value: 70, weight: 0.25 },
  { label: "Quality", value: 65, weight: 0.3 },
  { label: "Cost", value: 70, weight: 0.2 },
  { label: "Reliability", value: 72, weight: 0.15 },
  { label: "Availability", value: 80, weight: 0.1 },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function weightedOverallScore(dimensions: ScoreDimension[]): number {
  let totalWeight = 0;
  let weightedSum = 0;
  for (const d of dimensions) {
    totalWeight += d.weight;
    weightedSum += d.value * d.weight;
  }
  return totalWeight > 0 ? Math.round(weightedSum / totalWeight) : 0;
}

function scoreColor(score: number): string {
  if (score >= 80) return "text-emerald-600";
  if (score >= 60) return "text-brand-600";
  if (score >= 40) return "text-amber-600";
  return "text-red-600";
}

function scoreBgColor(score: number): string {
  if (score >= 80) return "bg-emerald-500";
  if (score >= 60) return "bg-brand-500";
  if (score >= 40) return "bg-amber-500";
  return "bg-red-500";
}

// ---------------------------------------------------------------------------
// Radar Chart (SVG)
// ---------------------------------------------------------------------------

function RadarChart({
  dimensions,
  categoryAvg,
  size = 200,
}: {
  dimensions: ScoreDimension[];
  categoryAvg?: ScoreDimension[];
  size?: number;
}) {
  const n = dimensions.length;
  if (n < 3) return null;

  const cx = size / 2;
  const cy = size / 2;
  const maxR = size * 0.38;

  const angleStep = (2 * Math.PI) / n;
  const startAngle = -Math.PI / 2; // Start from top

  function getPoint(index: number, value: number) {
    const angle = startAngle + index * angleStep;
    const r = (value / 100) * maxR;
    return {
      x: cx + r * Math.cos(angle),
      y: cy + r * Math.sin(angle),
    };
  }

  // Grid rings
  const rings = [20, 40, 60, 80, 100];

  // Build polygon points
  const modelPoints = dimensions.map((d, i) => getPoint(i, d.value));
  const modelPath = modelPoints.map((p) => `${p.x},${p.y}`).join(" ");

  const avgPoints = (categoryAvg ?? dimensions).map((d, i) => getPoint(i, d.value));
  const avgPath = avgPoints.map((p) => `${p.x},${p.y}`).join(" ");

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="mx-auto">
      {/* Grid rings */}
      {rings.map((ring) => (
        <polygon
          key={ring}
          points={Array.from({ length: n }, (_, i) => {
            const p = getPoint(i, ring);
            return `${p.x},${p.y}`;
          }).join(" ")}
          fill="none"
          stroke="var(--color-surface-200)"
          strokeWidth={0.5}
          opacity={0.6}
        />
      ))}

      {/* Axis lines */}
      {dimensions.map((_, i) => {
        const p = getPoint(i, 100);
        return (
          <line
            key={i}
            x1={cx}
            y1={cy}
            x2={p.x}
            y2={p.y}
            stroke="var(--color-surface-200)"
            strokeWidth={0.5}
            opacity={0.4}
          />
        );
      })}

      {/* Category average polygon (if provided and different) */}
      {categoryAvg && modelPath !== avgPath && (
        <polygon
          points={avgPath}
          fill="var(--color-surface-200)"
          fillOpacity={0.3}
          stroke="var(--color-surface-300)"
          strokeWidth={1}
          strokeDasharray="4 2"
        />
      )}

      {/* Model polygon */}
      <polygon
        points={modelPath}
        fill="var(--color-brand-500)"
        fillOpacity={0.2}
        stroke="var(--color-brand-500)"
        strokeWidth={2}
      />

      {/* Data points */}
      {modelPoints.map((p, i) => (
        <circle
          key={i}
          cx={p.x}
          cy={p.y}
          r={3}
          fill="var(--color-brand-600)"
          stroke="var(--color-surface-0)"
          strokeWidth={1.5}
        />
      ))}

      {/* Labels */}
      {dimensions.map((d, i) => {
        const angle = startAngle + i * angleStep;
        const labelR = maxR + 20;
        const x = cx + labelR * Math.cos(angle);
        const y = cy + labelR * Math.sin(angle);
        return (
          <text
            key={i}
            x={x}
            y={y}
            textAnchor="middle"
            dominantBaseline="central"
            fontSize={9}
            fill="var(--color-surface-800)"
            fontWeight={500}
            opacity={0.7}
          >
            {d.label}
          </text>
        );
      })}
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function ModelScoringCard({ data }: { data: ModelScoreData }) {
  const overallScore = useMemo(() => weightedOverallScore(data.dimensions), [data.dimensions]);
  const categoryAvg = useMemo(
    () => CATEGORY_AVERAGES[data.category] ?? DEFAULT_CATEGORY_AVG,
    [data.category],
  );
  const categoryOverall = useMemo(() => weightedOverallScore(categoryAvg), [categoryAvg]);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      {/* Header */}
      <div className="flex items-start justify-between mb-4">
        <div>
          <h3 className="font-semibold text-surface-900 text-lg">{data.modelName}</h3>
          <div className="flex items-center gap-2 mt-1">
            <span className="text-xs text-surface-800/40 bg-surface-100 rounded px-1.5 py-0.5">
              {data.tier}
            </span>
            <span className="text-xs text-surface-800/40 bg-surface-100 rounded px-1.5 py-0.5">
              {data.category}
            </span>
          </div>
        </div>
        {/* Overall score badge */}
        <div className="flex flex-col items-center">
          <div
            className={cn(
              "w-14 h-14 rounded-full flex items-center justify-center text-lg font-bold border-2",
              overallScore >= 80
                ? "bg-emerald-500/10 border-emerald-500 text-emerald-600"
                : overallScore >= 60
                  ? "bg-brand-500/10 border-brand-500 text-brand-600"
                  : overallScore >= 40
                    ? "bg-amber-500/10 border-amber-500 text-amber-600"
                    : "bg-red-500/10 border-red-500 text-red-600",
            )}
          >
            {overallScore}
          </div>
          <span className="text-[10px] text-surface-800/40 mt-1">Overall</span>
        </div>
      </div>

      {/* Radar Chart */}
      <div className="mb-4">
        <RadarChart dimensions={data.dimensions} categoryAvg={categoryAvg} size={220} />
      </div>

      {/* Score Breakdown */}
      <div className="space-y-2 mb-4">
        {data.dimensions.map((dim) => {
          const avgVal = categoryAvg.find((c) => c.label === dim.label)?.value ?? 50;
          const diff = dim.value - avgVal;
          return (
            <div key={dim.label}>
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-medium text-surface-800/60">{dim.label}</span>
                <div className="flex items-center gap-1.5">
                  <span className={cn("text-xs font-semibold", scoreColor(dim.value))}>
                    {dim.value}
                  </span>
                  {diff !== 0 && (
                    <span
                      className={cn(
                        "text-[10px] font-medium",
                        diff > 0 ? "text-emerald-500" : "text-red-500",
                      )}
                    >
                      {diff > 0 ? "+" : ""}
                      {diff} vs avg
                    </span>
                  )}
                </div>
              </div>
              <div className="relative h-1.5 bg-surface-100 rounded-full overflow-hidden">
                {/* Category average indicator */}
                <div
                  className="absolute top-0 h-full w-0.5 bg-surface-400 rounded-full z-10"
                  style={{ left: `${avgVal}%` }}
                  title={`Category avg: ${avgVal}`}
                />
                <div
                  className={cn("h-full rounded-full transition-all", scoreBgColor(dim.value))}
                  style={{ width: `${dim.value}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>

      {/* Category comparison */}
      <div className="flex items-center justify-between p-3 rounded-lg bg-surface-50 border border-surface-100">
        <div>
          <span className="text-xs text-surface-800/50 block">Category Average ({data.category})</span>
          <span className="text-sm font-semibold text-surface-800/70">{categoryOverall}</span>
        </div>
        <div className="text-right">
          <span className="text-xs text-surface-800/50 block">Your Score</span>
          <span className={cn("text-sm font-semibold", scoreColor(overallScore))}>
            {overallScore}
          </span>
        </div>
        <div
          className={cn(
            "text-xs font-bold px-2 py-0.5 rounded-full",
            overallScore >= categoryOverall
              ? "bg-emerald-500/10 text-emerald-600"
              : "bg-red-500/10 text-red-600",
          )}
        >
          {overallScore >= categoryOverall ? "+" : ""}
          {overallScore - categoryOverall}
        </div>
      </div>

      {/* Legend */}
      <div className="mt-3 flex items-center gap-4 text-[10px] text-surface-800/40">
        <div className="flex items-center gap-1">
          <span className="w-3 h-0.5 bg-brand-500 rounded inline-block" />
          This model
        </div>
        <div className="flex items-center gap-1">
          <span className="w-3 h-0.5 bg-surface-300 rounded inline-block border-dashed" />
          Category avg
        </div>
      </div>
    </div>
  );
}
