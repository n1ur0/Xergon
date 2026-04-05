'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── ModelsSkeleton ──
// Matches models page: header + filter tags + 8 model cards grid

export function ModelsSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <Skeleton variant="text" className="h-7 w-24" />
        <Skeleton variant="text" className="h-4 w-80 mt-1" />
      </div>

      {/* Filter tags */}
      <div className="flex flex-wrap gap-2 mb-6">
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
        <Skeleton variant="rect" className="h-8 w-16 rounded-full" />
      </div>

      {/* 8 model cards grid */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 8 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-5"
          >
            {/* Header: name + provider */}
            <div className="mb-2">
              <Skeleton variant="text" className="h-5 w-2/3 mb-1" />
              <Skeleton variant="text" className="h-3 w-24" />
            </div>

            {/* Description */}
            <Skeleton variant="text" className="h-4 w-full mb-1" />
            <Skeleton variant="text" className="h-4 w-3/4 mb-3" />

            {/* Metadata row */}
            <div className="flex items-center gap-4 mb-3">
              <Skeleton variant="text" className="h-4 w-20" />
              <Skeleton variant="text" className="h-4 w-16" />
            </div>

            {/* Tags */}
            <div className="flex flex-wrap gap-1 mb-4">
              <Skeleton variant="rect" className="h-5 w-14 rounded-full" />
              <Skeleton variant="rect" className="h-5 w-16 rounded-full" />
              <Skeleton variant="rect" className="h-5 w-14 rounded-full" />
            </div>

            {/* Footer */}
            <div className="flex items-center justify-between pt-3 border-t border-surface-100">
              <div className="space-y-1">
                <Skeleton variant="text" className="h-4 w-20" />
                <Skeleton variant="text" className="h-3 w-28" />
              </div>
              <Skeleton variant="rect" className="h-8 w-16 rounded-lg" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default ModelsSkeleton;
