'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── ExplorerSkeleton ──
// Matches explorer page: header + filter bar + 6 provider cards

export function ExplorerSkeleton() {
  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Page header */}
      <div className="space-y-1">
        <Skeleton variant="text" className="h-7 w-48" />
        <Skeleton variant="text" className="h-4 w-72" />
      </div>

      {/* Filter bar */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
        <div className="flex flex-wrap gap-3">
          <Skeleton variant="rect" className="h-9 w-56 rounded-lg" />
          <Skeleton variant="rect" className="h-9 w-28 rounded-lg" />
          <Skeleton variant="rect" className="h-9 w-28 rounded-lg" />
          <Skeleton variant="rect" className="h-9 w-28 rounded-lg" />
        </div>
      </div>

      {/* Status bar */}
      <div className="flex items-center justify-between">
        <Skeleton variant="text" className="h-4 w-40" />
        <Skeleton variant="text" className="h-4 w-24" />
      </div>

      {/* 6 provider cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3"
          >
            {/* Header row */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Skeleton variant="circle" className="h-2.5 w-2.5" />
                <Skeleton variant="text" className="h-4 w-28" />
              </div>
              <Skeleton variant="rect" className="h-5 w-16 rounded-full" />
            </div>
            {/* Subtitle */}
            <Skeleton variant="text" className="h-3 w-48" />
            {/* Tags */}
            <div className="flex gap-1">
              <Skeleton variant="rect" className="h-5 w-20 rounded-md" />
              <Skeleton variant="rect" className="h-5 w-24 rounded-md" />
              <Skeleton variant="rect" className="h-5 w-16 rounded-md" />
            </div>
            {/* Progress bar */}
            <Skeleton variant="rect" className="h-1.5 w-full rounded-full" />
            {/* Stats grid */}
            <div className="grid grid-cols-3 gap-2">
              <Skeleton variant="rect" className="h-12 rounded-lg" />
              <Skeleton variant="rect" className="h-12 rounded-lg" />
              <Skeleton variant="rect" className="h-12 rounded-lg" />
            </div>
            {/* Footer */}
            <Skeleton variant="text" className="h-3 w-full" />
          </div>
        ))}
      </div>
    </main>
  );
}

export default ExplorerSkeleton;
