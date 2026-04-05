"use client";

import { useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface UptimeBarProps {
  serviceName: string;
  /** 7 values, one per day (0-100). Index 0 = 6 days ago, index 6 = today. */
  dailyUptime: number[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getDayLabels(): string[] {
  const days: string[] = [];
  const now = new Date();
  for (let i = 6; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    days.push(d.toLocaleDateString("en-US", { weekday: "short" }));
  }
  return days;
}

function segmentColor(uptime: number): string {
  if (uptime >= 99) return "bg-emerald-500";
  if (uptime >= 95) return "bg-amber-500";
  if (uptime > 0) return "bg-red-500";
  return "bg-surface-200";
}

function segmentHoverColor(uptime: number): string {
  if (uptime >= 99) return "bg-emerald-400";
  if (uptime >= 95) return "bg-amber-400";
  if (uptime > 0) return "bg-red-400";
  return "bg-surface-300";
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function UptimeBar({ serviceName, dailyUptime }: UptimeBarProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const dayLabels = getDayLabels();
  const data = dailyUptime.length === 7 ? dailyUptime : Array(7).fill(100);

  return (
    <div className="space-y-1.5">
      <span className="text-xs font-medium text-surface-800/60">{serviceName}</span>
      <div className="relative flex gap-1">
        {data.map((uptime, i) => (
          <div
            key={i}
            className="relative flex-1 h-5 rounded-sm cursor-pointer group"
            onMouseEnter={() => setHoveredIndex(i)}
            onMouseLeave={() => setHoveredIndex(null)}
          >
            <div
              className={`h-full w-full rounded-sm transition-colors ${
                hoveredIndex === i
                  ? segmentHoverColor(uptime)
                  : segmentColor(uptime)
              }`}
              style={{ opacity: uptime > 0 ? 1 : 0.3 }}
            />

            {/* Tooltip */}
            {hoveredIndex === i && (
              <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 rounded-md bg-surface-900 text-white text-[10px] font-medium whitespace-nowrap z-10 pointer-events-none">
                {dayLabels[i]}: {uptime.toFixed(1)}%
                <div className="absolute top-full left-1/2 -translate-x-1/2 -mt-px border-4 border-transparent border-t-surface-900" />
              </div>
            )}
          </div>
        ))}

        {/* Day labels */}
        {data.map((_, i) => (
          <div
            key={`label-${i}`}
            className="flex-1 text-center text-[9px] text-surface-800/30 mt-0.5"
          >
            {dayLabels[i]}
          </div>
        ))}
      </div>
    </div>
  );
}
