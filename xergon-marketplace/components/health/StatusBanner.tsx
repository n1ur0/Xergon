"use client";

// ---------------------------------------------------------------------------
// StatusBanner — full-width banner showing overall system status
// ---------------------------------------------------------------------------

interface StatusBannerProps {
  overall: "operational" | "degraded" | "partial" | "major_outage";
  lastUpdated: string;
}

const STATUS_CONFIG = {
  operational: {
    message: "All Systems Operational",
    bg: "bg-emerald-50 dark:bg-emerald-950/20",
    border: "border-emerald-200 dark:border-emerald-800/40",
    text: "text-emerald-800 dark:text-emerald-300",
    dotColor: "bg-emerald-500",
    pulse: false,
  },
  degraded: {
    message: "Minor Degradation Detected",
    bg: "bg-amber-50 dark:bg-amber-950/20",
    border: "border-amber-200 dark:border-amber-800/40",
    text: "text-amber-800 dark:text-amber-300",
    dotColor: "bg-amber-500",
    pulse: true,
  },
  partial: {
    message: "Partial Degradation Detected",
    bg: "bg-amber-50 dark:bg-amber-950/20",
    border: "border-amber-200 dark:border-amber-800/40",
    text: "text-amber-800 dark:text-amber-300",
    dotColor: "bg-amber-500",
    pulse: true,
  },
  major_outage: {
    message: "Major Service Outage",
    bg: "bg-red-50 dark:bg-red-950/20",
    border: "border-red-200 dark:border-red-800/40",
    text: "text-red-800 dark:text-red-300",
    dotColor: "bg-red-500",
    pulse: true,
  },
} as const;

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}

export function StatusBanner({ overall, lastUpdated }: StatusBannerProps) {
  const config = STATUS_CONFIG[overall];

  return (
    <div
      className={`rounded-xl border ${config.border} ${config.bg} px-5 py-4 flex items-center justify-between gap-4`}
    >
      <div className="flex items-center gap-3">
        <span className="relative flex h-3 w-3">
          {config.pulse && (
            <span
              className={`absolute inline-flex h-full w-full rounded-full ${config.dotColor} opacity-75 animate-ping`}
            />
          )}
          <span
            className={`relative inline-flex h-3 w-3 rounded-full ${config.dotColor}`}
          />
        </span>
        <span className={`text-sm font-semibold ${config.text}`}>
          {config.message}
        </span>
      </div>
      <span className={`text-xs ${config.text} opacity-60`}>
        Updated {timeAgo(lastUpdated)}
      </span>
    </div>
  );
}
