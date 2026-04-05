"use client";

import { useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RegionDistributionProps {
  regions: Record<string, number>;
}

// ---------------------------------------------------------------------------
// Region color palette
// ---------------------------------------------------------------------------

const REGION_COLORS: Record<string, string> = {
  "North America": "bg-blue-500",
  Europe: "bg-violet-500",
  Asia: "bg-amber-500",
  "South America": "bg-emerald-500",
  Oceania: "bg-cyan-500",
  Africa: "bg-rose-500",
};

const REGION_DARK_COLORS: Record<string, string> = {
  "North America": "bg-blue-400",
  Europe: "bg-violet-400",
  Asia: "bg-amber-400",
  "South America": "bg-emerald-400",
  Oceania: "bg-cyan-400",
  Africa: "bg-rose-400",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function RegionDistribution({ regions }: RegionDistributionProps) {
  const entries = useMemo(() => {
    return Object.entries(regions).sort(([, a], [, b]) => b - a);
  }, [regions]);

  const total = useMemo(() => entries.reduce((s, [, v]) => s + v, 0), [entries]);

  if (entries.length === 0 || total === 0) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 text-center text-surface-800/50 text-sm">
        No region data available.
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h2 className="text-base font-semibold text-surface-900 mb-1">
        Provider Distribution
      </h2>
      <p className="text-xs text-surface-800/40 mb-4">
        {total} providers across {entries.length} regions
      </p>

      <div className="space-y-3">
        {entries.map(([region, count]) => {
          const pct = ((count / total) * 100).toFixed(1);
          const lightColor = REGION_COLORS[region] ?? "bg-surface-300";
          const darkColor = REGION_DARK_COLORS[region] ?? "bg-surface-400";

          return (
            <div key={region}>
              <div className="flex items-center justify-between mb-1">
                <span className="text-sm text-surface-800/70">{region}</span>
                <span className="text-xs text-surface-800/40 font-mono">
                  {count} ({pct}%)
                </span>
              </div>
              <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
                <div
                  className={`h-full rounded-full ${lightColor} dark:${darkColor} transition-all duration-700`}
                  style={{ width: `${pct}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
