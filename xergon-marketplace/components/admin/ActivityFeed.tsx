"use client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ActivityItem {
  type: "provider_registered" | "rental_started" | "rental_completed" | "withdrawal" | "dispute_opened";
  description: string;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(diff / 60_000);
  const hours = Math.floor(diff / 3_600_000);
  const days = Math.floor(diff / 86_400_000);

  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days < 7) return `${days}d ago`;
  return new Date(iso).toLocaleDateString();
}

// ---------------------------------------------------------------------------
// Icons per activity type
// ---------------------------------------------------------------------------

function ActivityIcon({ type }: { type: ActivityItem["type"] }) {
  const base = "w-4 h-4 flex-shrink-0";

  switch (type) {
    case "provider_registered":
      return (
        <svg className={`${base} text-accent-500`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2" />
          <circle cx="9" cy="7" r="4" />
          <line x1="19" y1="8" x2="19" y2="14" />
          <line x1="22" y1="11" x2="16" y2="11" />
        </svg>
      );
    case "rental_started":
      return (
        <svg className={`${base} text-brand-500`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polygon points="5 3 19 12 5 21 5 3" />
        </svg>
      );
    case "rental_completed":
      return (
        <svg className={`${base} text-emerald-500`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
          <polyline points="22 4 12 14.01 9 11.01" />
        </svg>
      );
    case "withdrawal":
      return (
        <svg className={`${base} text-amber-500`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="12" y1="1" x2="12" y2="23" />
          <path d="M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6" />
        </svg>
      );
    case "dispute_opened":
      return (
        <svg className={`${base} text-danger-500`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
      );
    default:
      return null;
  }
}

function typeColor(type: ActivityItem["type"]): string {
  switch (type) {
    case "provider_registered": return "bg-accent-50 dark:bg-accent-950/20";
    case "rental_started": return "bg-brand-50 dark:bg-brand-950/20";
    case "rental_completed": return "bg-emerald-50 dark:bg-emerald-950/20";
    case "withdrawal": return "bg-amber-50 dark:bg-amber-950/20";
    case "dispute_opened": return "bg-danger-50 dark:bg-danger-950/20";
    default: return "bg-surface-100";
  }
}

// ---------------------------------------------------------------------------
// ActivityFeed
// ---------------------------------------------------------------------------

export function ActivityFeed({ activities }: { activities: ActivityItem[] }) {
  if (activities.length === 0) {
    return (
      <div className="text-center py-8 text-surface-800/40">
        No recent activity
      </div>
    );
  }

  return (
    <div className="space-y-2 max-h-[400px] overflow-y-auto pr-1">
      {activities.map((activity, i) => (
        <div
          key={`${activity.type}-${i}`}
          className={`flex items-start gap-3 p-3 rounded-lg ${typeColor(activity.type)} transition-colors`}
        >
          <div className="mt-0.5">
            <ActivityIcon type={activity.type} />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm text-surface-800/80 leading-snug">
              {activity.description}
            </p>
            <p className="text-xs text-surface-800/40 mt-1">
              {relativeTime(activity.timestamp)}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}
