"use client";

import type { ReactNode } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface StatItem {
  label: string;
  value: string;
  icon?: ReactNode;
  trend?: "up" | "down" | "neutral";
}

// ---------------------------------------------------------------------------
// Icons (inline SVGs matching project pattern)
// ---------------------------------------------------------------------------

function UpArrow() {
  return (
    <svg className="w-3.5 h-3.5 text-accent-500" viewBox="0 0 20 20" fill="currentColor">
      <path
        fillRule="evenodd"
        d="M12 7a1 1 0 10-2 0v3.586L8.707 9.293a1 1 0 00-1.414 1.414l3 3a1 1 0 001.414 0l3-3a1 1 0 00-1.414-1.414L12 10.586V7z"
        clipRule="evenodd"
      />
    </svg>
  );
}

function DownArrow() {
  return (
    <svg className="w-3.5 h-3.5 text-danger-500" viewBox="0 0 20 20" fill="currentColor">
      <path
        fillRule="evenodd"
        d="M12 13a1 1 0 100-2V7.414L13.293 8.707a1 1 0 001.414-1.414l-3-3a1 1 0 00-1.414 0l-3 3a1 1 0 101.414 1.414L8 7.414V11a1 1 0 100 2h4z"
        clipRule="evenodd"
      />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// StatCard
// ---------------------------------------------------------------------------

function StatCard({ stat, index }: { stat: StatItem; index: number }) {
  return (
    <div
      className="rounded-xl border border-surface-200 bg-surface-0 p-5 transition-all hover:shadow-md"
      style={{
        animationDelay: `${index * 60}ms`,
        animation: "fadeIn 0.4s ease-out both",
      }}
    >
      <div className="flex items-start justify-between mb-3">
        {stat.icon && (
          <div
            className="rounded-lg bg-brand-50 p-2 text-brand-600 dark:bg-brand-950/30"
            aria-hidden="true"
          >
            {stat.icon}
          </div>
        )}
        {stat.trend && stat.trend !== "neutral" && (
          <div className="flex items-center gap-0.5 text-xs font-medium" aria-hidden="true">
            {stat.trend === "up" ? <UpArrow /> : <DownArrow />}
          </div>
        )}
      </div>
      <div className="text-2xl font-bold text-surface-900 mb-0.5">{stat.value}</div>
      <div className="text-sm text-surface-800/50">{stat.label}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// StatsGrid
// ---------------------------------------------------------------------------

export function StatsGrid({ stats }: { stats: StatItem[] }) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4">
      {stats.map((stat, i) => (
        <StatCard key={stat.label} stat={stat} index={i} />
      ))}
    </div>
  );
}
