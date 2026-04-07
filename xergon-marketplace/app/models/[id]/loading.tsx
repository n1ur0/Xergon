import { Skeleton } from "@/components/ui/Skeleton";

export default function ModelDetailLoading() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Back link */}
      <Skeleton className="h-4 w-32 mb-6" />

      {/* Header */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
        <div className="flex flex-col sm:flex-row sm:items-start gap-4">
          <Skeleton className="h-16 w-16 rounded-full shrink-0" />
          <div className="flex-1 min-w-0 space-y-2">
            <Skeleton className="h-7 w-64" />
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-4 w-80" />
          </div>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6">
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="rounded-lg border border-surface-200 bg-surface-0 p-4">
            <Skeleton className="h-3 w-24 mb-2" />
            <Skeleton className="h-6 w-16" />
          </div>
        ))}
      </div>

      {/* Playground section */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 mb-6">
        <Skeleton className="h-10 w-32 mx-4 mt-4" />
        <div className="p-4">
          <Skeleton className="h-64 w-full rounded-lg" />
        </div>
      </div>

      {/* Reviews + Related */}
      <div className="grid gap-6 lg:grid-cols-2">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <Skeleton className="h-6 w-24 mb-4" />
          <div className="space-y-3">
            {[1, 2, 3].map((i) => (
              <div key={i} className="rounded-lg border border-surface-100 p-3">
                <Skeleton className="h-4 w-32 mb-2" />
                <Skeleton className="h-3 w-full" />
                <Skeleton className="h-3 w-3/4 mt-1" />
              </div>
            ))}
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <Skeleton className="h-6 w-32 mb-4" />
          <div className="space-y-3">
            {[1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-16 w-full rounded-lg" />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
