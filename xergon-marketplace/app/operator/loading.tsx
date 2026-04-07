export default function Loading() {
  return (
    <div className="flex-1 min-w-0 px-4 py-6 lg:px-8 space-y-6 animate-pulse">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <div className="h-7 w-40 rounded bg-surface-200 dark:bg-surface-700 mb-2" />
          <div className="h-4 w-56 rounded bg-surface-200 dark:bg-surface-700" />
        </div>
        <div className="h-9 w-24 rounded-lg bg-surface-200 dark:bg-surface-700" />
      </div>

      {/* Stat cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-5"
          >
            <div className="h-5 w-5 rounded bg-surface-200 dark:bg-surface-700 mb-3" />
            <div className="h-7 w-24 rounded bg-surface-200 dark:bg-surface-700 mb-1" />
            <div className="h-3 w-16 rounded bg-surface-200 dark:bg-surface-700" />
          </div>
        ))}
      </div>

      {/* Content area */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
          <div className="h-5 w-28 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
          <div className="space-y-3">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="h-12 w-full rounded-lg bg-surface-200 dark:bg-surface-700" />
            ))}
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
          <div className="h-5 w-28 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
          <div className="space-y-3">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="h-12 w-full rounded-lg bg-surface-200 dark:bg-surface-700" />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
