"use client";

import { useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EarningsChartProps {
  data: Array<{ date: string; value: number }>;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function EarningsChart({ data }: EarningsChartProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-[260px] text-sm text-surface-800/40">
        No earnings data available
      </div>
    );
  }

  const maxValue = Math.max(...data.map((d) => d.value), 1);

  // Format date as short month/day
  function formatDate(dateStr: string): string {
    const d = new Date(dateStr + "T00:00:00");
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }

  // Format nanoERG to ERG with 2 decimals
  function formatValue(nanoErg: number): string {
    return (nanoErg / 1e9).toFixed(2) + " ERG";
  }

  return (
    <div className="relative">
      {/* Y-axis labels + bars */}
      <div className="flex items-end gap-1 h-[260px]">
        {data.map((d, i) => {
          const height = Math.max((d.value / maxValue) * 100, 2);
          const isHovered = hoveredIndex === i;

          return (
            <div
              key={d.date}
              className="flex-1 flex flex-col items-center justify-end h-full relative group"
              onMouseEnter={() => setHoveredIndex(i)}
              onMouseLeave={() => setHoveredIndex(null)}
            >
              {/* Tooltip */}
              {isHovered && (
                <div className="absolute -top-16 left-1/2 -translate-x-1/2 z-10 whitespace-nowrap rounded-lg border border-surface-200 bg-surface-0 px-3 py-1.5 text-xs shadow-lg pointer-events-none">
                  <div className="font-semibold text-surface-900">
                    {formatValue(d.value)}
                  </div>
                  <div className="text-surface-800/50">{formatDate(d.date)}</div>
                  {/* Tooltip arrow */}
                  <div className="absolute -bottom-1 left-1/2 -translate-x-1/2 w-2 h-2 rotate-45 border-r border-b border-surface-200 bg-surface-0" />
                </div>
              )}

              {/* Bar */}
              <div
                className={`w-full rounded-t-sm transition-all duration-150 min-w-[3px] ${
                  isHovered
                    ? "bg-brand-500"
                    : "bg-brand-400/70 dark:bg-brand-600/50"
                }`}
                style={{ height: `${height}%` }}
              />
            </div>
          );
        })}
      </div>

      {/* X-axis labels (show every 5th) */}
      <div className="flex mt-2">
        {data.map((d, i) => (
          <div
            key={d.date}
            className="flex-1 text-center text-[10px] text-surface-800/30"
          >
            {i % 5 === 0 ? formatDate(d.date) : ""}
          </div>
        ))}
      </div>
    </div>
  );
}
