"use client";

import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SummaryCardProps {
  title: string;
  value: string;
  subtitle?: string;
  icon?: React.ReactNode;
  trend?: "up" | "down" | "neutral";
  trendValue?: string;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SummaryCard({
  title,
  value,
  subtitle,
  icon,
  trend,
  trendValue,
}: SummaryCardProps) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm transition-shadow hover:shadow-md">
      {/* Header row: icon + title */}
      <div className="flex items-center justify-between mb-3">
        <span className="text-sm font-medium text-surface-800/60">
          {title}
        </span>
        {icon && (
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-brand-50 text-brand-600 dark:bg-brand-950/40 dark:text-brand-400">
            {icon}
          </div>
        )}
      </div>

      {/* Value */}
      <div className="text-2xl font-bold text-surface-900 tracking-tight">
        {value}
      </div>

      {/* Subtitle / Trend row */}
      <div className="flex items-center gap-2 mt-1">
        {trend && trendValue && (
          <span
            className={cn(
              "inline-flex items-center gap-0.5 text-xs font-medium",
              trend === "up" && "text-emerald-600 dark:text-emerald-400",
              trend === "down" && "text-red-600 dark:text-red-400",
              trend === "neutral" && "text-surface-800/40",
            )}
          >
            {trend === "up" && (
              <svg
                className="h-3 w-3"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={2.5}
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M4.5 19.5l15-15m0 0H8.25m11.25 0v11.25"
                />
              </svg>
            )}
            {trend === "down" && (
              <svg
                className="h-3 w-3"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={2.5}
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M4.5 4.5l15 15m0 0V8.25m0 11.25H8.25"
                />
              </svg>
            )}
            {trendValue}
          </span>
        )}
        {subtitle && (
          <span className="text-xs text-surface-800/40">{subtitle}</span>
        )}
      </div>
    </div>
  );
}
