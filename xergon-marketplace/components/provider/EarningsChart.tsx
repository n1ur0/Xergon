"use client";

import { useState, useMemo, useRef, useEffect } from "react";
import type { SettlementRecord } from "@/lib/api/provider";
import {
  generateBarPath,
  calculateYAxisTicks,
  formatAxisValue,
  generateSparkline,
} from "@/lib/utils/charts";

interface EarningsChartProps {
  settlements: SettlementRecord[];
}

interface DailyEarning {
  date: string; // YYYY-MM-DD
  erg: number;
  label: string; // short display like "Mar 5"
}

function groupByDay(settlements: SettlementRecord[]): DailyEarning[] {
  const map = new Map<string, number>();

  // Generate last 30 days
  const days: DailyEarning[] = [];
  const now = new Date();
  for (let i = 29; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    const key = d.toISOString().slice(0, 10);
    const label = d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
    days.push({ date: key, erg: 0, label });
    map.set(key, 0);
  }

  // Sum confirmed + pending settlements by day
  for (const tx of settlements) {
    const key = tx.createdAt.slice(0, 10);
    if (map.has(key)) {
      map.set(key, map.get(key)! + tx.amountErg);
    }
  }

  for (const day of days) {
    day.erg = map.get(day.date) ?? 0;
  }

  return days;
}

export function EarningsChart({ settlements }: EarningsChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setWidth(Math.max(300, Math.floor(entry.contentRect.width)));
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const days = useMemo(() => groupByDay(settlements), [settlements]);
  const hasData = days.some((d) => d.erg > 0);

  const height = 220;
  const padding = { top: 24, right: 16, bottom: 36, left: 56 };
  const plotW = width - padding.left - padding.right;
  const plotH = height - padding.top - padding.bottom;

  const values = days.map((d) => d.erg);
  const maxVal = Math.max(...values, 0.01);
  const totalErg = values.reduce((a, b) => a + b, 0);
  const avgErg = totalErg / 30;
  const ticks = calculateYAxisTicks(0, maxVal, 5);

  const barPath = hasData ? generateBarPath(values, width, height, padding, 2) : "";

  // Compute bar positions for hover detection
  const barCount = days.length;
  const barGap = Math.max(1, Math.round(plotW / barCount * 0.2));
  const barWidth = Math.max(2, (plotW - barGap * (barCount + 1)) / barCount);
  const baseline = padding.top + plotH;
  const range = maxVal || 1;

  const avgY = hasData
    ? padding.top + plotH - (avgErg / range) * plotH
    : 0;

  const totalY = hasData
    ? padding.top + plotH - (maxVal / range) * plotH
    : 0;

  if (!hasData) {
    return (
      <div ref={containerRef} className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-4 flex items-center gap-2">
          <span className="text-lg">&#x1F4C8;</span> Earnings (30d)
        </h2>
        <div className="flex items-center justify-center py-12 text-surface-800/40 text-sm">
          <div className="text-center">
            <div className="text-3xl mb-2 opacity-30">&#x1F4B0;</div>
            <p>No settlement data yet.</p>
            <p className="text-xs mt-1">ERG earnings will appear here once settlements are confirmed.</p>
          </div>
        </div>
      </div>
    );
  }

  const hovered = hoverIndex !== null ? days[hoverIndex] : null;

  return (
    <div ref={containerRef} className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-semibold flex items-center gap-2">
          <span className="text-lg">&#x1F4C8;</span> Earnings (30d)
        </h2>
        <div className="flex items-center gap-4 text-xs text-surface-800/50">
          <span className="flex items-center gap-1.5">
            <span className="inline-block w-3 h-0.5 bg-brand-500 rounded" />
            Total: {formatAxisValue(totalErg, "erg")}
          </span>
          <span className="flex items-center gap-1.5">
            <span className="inline-block w-3 h-0.5 border-t border-dashed border-surface-400" />
            Avg: {formatAxisValue(avgErg, "erg")}/day
          </span>
        </div>
      </div>

      {/* Tooltip */}
      {hovered && (
        <div
          className="absolute z-10 px-3 py-2 rounded-lg bg-surface-900 text-white text-xs shadow-lg pointer-events-none"
          style={{
            left: Math.min(
              Math.max(
                padding.left + barGap + (hoverIndex ?? 0) * (barWidth + barGap) + barWidth / 2 - 40,
                0,
              ),
              width - 120,
            ),
            top: Math.max(
              padding.top + plotH - ((hovered?.erg ?? 0 / range) * plotH) - 48,
              8,
            ),
          }}
        >
          <div className="font-medium">{formatAxisValue(hovered.erg, "erg")}</div>
          <div className="text-surface-300">{hovered.date}</div>
        </div>
      )}

      <div className="relative">
        <svg width={width} height={height} className="block">
          {/* Y-axis grid lines and labels */}
          {ticks.map((tick, i) => {
            const y = padding.top + plotH - (tick / range) * plotH;
            return (
              <g key={i}>
                <line
                  x1={padding.left}
                  y1={y}
                  x2={width - padding.right}
                  y2={y}
                  stroke="currentColor"
                  className="text-surface-100"
                  strokeWidth={1}
                />
                <text
                  x={padding.left - 8}
                  y={y + 4}
                  textAnchor="end"
                  className="fill-surface-800/50"
                  fontSize={10}
                >
                  {formatAxisValue(tick, "erg")}
                </text>
              </g>
            );
          })}

          {/* Average dashed line */}
          {hasData && avgErg > 0 && (
            <line
              x1={padding.left}
              y1={avgY}
              x2={width - padding.right}
              y2={avgY}
              className="stroke-surface-400"
              strokeWidth={1}
              strokeDasharray="4 3"
            />
          )}

          {/* Bars */}
          <defs>
            <linearGradient id="barGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--color-brand-500, #6366f1)" />
              <stop offset="100%" stopColor="var(--color-brand-600, #4f46e5)" />
            </linearGradient>
          </defs>
          {days.map((day, i) => {
            const x = padding.left + barGap + i * (barWidth + barGap);
            const barH = Math.max((day.erg / range) * plotH, 0);
            const y = baseline - barH;
            const isHovered = hoverIndex === i;

            return (
              <g
                key={day.date}
                onMouseEnter={() => setHoverIndex(i)}
                onMouseLeave={() => setHoverIndex(null)}
                className="cursor-pointer"
              >
                {/* Invisible hit area */}
                <rect
                  x={x - 1}
                  y={padding.top}
                  width={barWidth + 2}
                  height={plotH}
                  fill="transparent"
                />
                {/* Bar */}
                <rect
                  x={x}
                  y={y}
                  width={barWidth}
                  height={barH}
                  rx={2}
                  fill="url(#barGrad)"
                  opacity={isHovered ? 1 : 0.8}
                  className="transition-opacity"
                />
              </g>
            );
          })}

          {/* X-axis labels (every 5th day) */}
          {days.map((day, i) => {
            if (i % 5 !== 0 && i !== days.length - 1) return null;
            const x = padding.left + barGap + i * (barWidth + barGap) + barWidth / 2;
            return (
              <text
                key={day.date}
                x={x}
                y={height - padding.bottom + 16}
                textAnchor="middle"
                className="fill-surface-800/50"
                fontSize={9}
              >
                {day.label}
              </text>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
