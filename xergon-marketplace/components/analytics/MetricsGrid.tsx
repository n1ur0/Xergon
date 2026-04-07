"use client";

import { useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TrendDirection = "up" | "down" | "stable";

interface MetricItem {
  label: string;
  value: string;
  /** Raw numeric value for sparkline */
  rawValue: number;
  /** Sparkline data points (recent values) */
  sparkline?: number[];
  /** Trend compared to previous period */
  trend?: TrendDirection;
  /** Trend percentage change */
  trendPct?: number;
  /** Icon element */
  icon: React.ReactNode;
}

interface MetricsGridProps {
  metrics: MetricItem[];
  columns?: 2 | 3 | 4;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function TrendArrow({ direction }: { direction: TrendDirection }) {
  const color =
    direction === "up"
      ? "text-emerald-600 dark:text-emerald-400"
      : direction === "down"
      ? "text-red-500 dark:text-red-400"
      : "text-surface-800/30 dark:text-surface-200/30";

  return (
    <span className={`inline-flex items-center text-xs font-medium ${color}`}>
      {direction === "up" && (
        <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M12 7a1 1 0 10-2 0v3.586L8.707 9.293a1 1 0 00-1.414 1.414l3 3a1 1 0 001.414 0l3-3a1 1 0 00-1.414-1.414L12 10.586V7z" clipRule="evenodd" />
        </svg>
      )}
      {direction === "down" && (
        <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M12 13a1 1 0 100-2V7.414L13.293 8.707a1 1 0 001.414-1.414l-3-3a1 1 0 00-1.414 0l-3 3a1 1 0 101.414 1.414L8 7.414V11a1 1 0 100 2h4z" clipRule="evenodd" />
        </svg>
      )}
      {direction === "stable" && (
        <svg className="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M3 10a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" clipRule="evenodd" />
        </svg>
      )}
    </span>
  );
}

function MiniSparkline({ data }: { data: number[] }) {
  if (data.length < 2) return null;

  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const width = 80;
  const height = 28;
  const padding = 2;

  const points = data
    .map((v, i) => {
      const x = padding + (i / (data.length - 1)) * (width - padding * 2);
      const y = padding + (1 - (v - min) / range) * (height - padding * 2);
      return `${x},${y}`;
    })
    .join(" ");

  // Determine color based on trend
  const isUp = data[data.length - 1] >= data[0];
  const color = isUp ? "#10b981" : "#ef4444";

  return (
    <svg viewBox={`0 0 ${width} ${height}`} className="w-20 h-7" preserveAspectRatio="none">
      <polyline
        points={points}
        fill="none"
        stroke={color}
        strokeWidth="1.5"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function MetricsGrid({ metrics, columns = 4 }: MetricsGridProps) {
  const gridCols = {
    2: "grid-cols-2",
    3: "grid-cols-3",
    4: "grid-cols-2 md:grid-cols-4",
  };

  return (
    <div className={`grid ${gridCols[columns]} gap-4`}>
      {metrics.map((metric) => (
        <MetricCard key={metric.label} metric={metric} />
      ))}
    </div>
  );
}

function MetricCard({ metric }: { metric: MetricItem }) {
  const sparkData = useMemo(() => metric.sparkline ?? [], [metric.sparkline]);

  return (
    <div
      className="rounded-xl border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 p-4 transition-all hover:shadow-md"
      role="region"
      aria-label={`${metric.label}: ${metric.value}`}
    >
      <div className="flex items-start justify-between mb-2">
        <div className="rounded-lg bg-brand-50 p-2 text-brand-600 dark:bg-brand-950/30">
          {metric.icon}
        </div>
        <div className="flex flex-col items-end gap-1">
          {metric.trend && <TrendArrow direction={metric.trend} />}
          {metric.trendPct !== undefined && (
            <span
              className={`text-[10px] font-medium ${
                metric.trendPct >= 0
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-red-500 dark:text-red-400"
              }`}
            >
              {metric.trendPct >= 0 ? "+" : ""}
              {metric.trendPct.toFixed(1)}%
            </span>
          )}
        </div>
      </div>

      <div className="text-xl font-bold text-surface-900 dark:text-surface-0 mb-0.5">
        {metric.value}
      </div>

      <div className="flex items-center justify-between">
        <span className="text-xs text-surface-800/50">{metric.label}</span>
        {sparkData.length >= 2 && <MiniSparkline data={sparkData} />}
      </div>
    </div>
  );
}
