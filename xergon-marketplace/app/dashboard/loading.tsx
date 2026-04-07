import { PageSkeleton } from "@/components/ui/PageSkeleton";

export default function Loading() {
  return (
    <div className="mx-auto max-w-6xl px-4 py-8 space-y-6">
      {/* Header skeleton */}
      <div className="flex items-center justify-between">
        <div className="space-y-2">
          <div className="h-7 w-48 rounded bg-surface-200 dark:bg-surface-700 animate-pulse" />
          <div className="h-4 w-72 rounded bg-surface-200 dark:bg-surface-700 animate-pulse" />
        </div>
        <div className="h-9 w-24 rounded-lg bg-surface-200 dark:bg-surface-700 animate-pulse" />
      </div>

      {/* Stat cards skeleton */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-5 animate-pulse"
          >
            <div className="flex items-center justify-between mb-3">
              <div className="h-5 w-5 rounded bg-surface-200 dark:bg-surface-700" />
              <div className="h-4 w-16 rounded-full bg-surface-200 dark:bg-surface-700" />
            </div>
            <div className="h-7 w-28 rounded bg-surface-200 dark:bg-surface-700 mb-2" />
            <div className="h-3 w-20 rounded bg-surface-200 dark:bg-surface-700" />
          </div>
        ))}
      </div>

      {/* Chart + Activity skeleton */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
          <div className="flex items-center justify-between mb-6">
            <div className="h-5 w-32 rounded bg-surface-200 dark:bg-surface-700" />
            <div className="flex gap-2">
              <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
              <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
              <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
            </div>
          </div>
          <div className="flex items-end gap-1 h-48">
            {Array.from({ length: 30 }).map((_, i) => (
              <div
                key={i}
                className="flex-1 rounded-t bg-surface-200 dark:bg-surface-700"
                style={{ height: `${Math.random() * 80 + 10}%` }}
              />
            ))}
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
          <div className="h-5 w-32 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
          <div className="space-y-3">
            {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="flex gap-3">
                <div className="h-8 w-8 rounded-full bg-surface-200 dark:bg-surface-700 shrink-0" />
                <div className="flex-1 space-y-1.5">
                  <div className="h-3.5 w-3/4 rounded bg-surface-200 dark:bg-surface-700" />
                  <div className="h-3 w-1/2 rounded bg-surface-200 dark:bg-surface-700" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Table skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
        <div className="h-5 w-40 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
        <div className="space-y-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="h-10 w-full rounded-lg bg-surface-200 dark:bg-surface-700" />
          ))}
        </div>
      </div>
    </div>
  );
}
