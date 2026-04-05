'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── LeaderboardSkeleton ──
// Matches leaderboard page: table with 10 row skeletons

export function LeaderboardSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <Skeleton variant="text" className="h-7 w-52" />
        <Skeleton variant="text" className="h-4 w-96 mt-1" />
      </div>

      {/* Desktop table skeleton */}
      <div className="hidden md:block overflow-x-auto rounded-xl border border-surface-200 bg-surface-0">
        {/* Table header */}
        <div className="border-b border-surface-200 bg-surface-50 px-4 py-3">
          <div className="flex items-center gap-4">
            <Skeleton variant="text" className="h-3 w-12 uppercase" />
            <Skeleton variant="text" className="h-3 w-16 uppercase" />
            <Skeleton variant="text" className="h-3 w-14 uppercase" />
            <Skeleton variant="text" className="h-3 w-16 uppercase" />
            <Skeleton variant="text" className="h-3 w-20 uppercase" />
            <Skeleton variant="text" className="h-3 w-16 uppercase" />
            <Skeleton variant="text" className="h-3 w-20 uppercase" />
          </div>
        </div>

        {/* Table rows */}
        <div className="divide-y divide-surface-100">
          {Array.from({ length: 10 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 px-4 py-3.5">
              {/* Rank */}
              <Skeleton variant="circle" className="h-7 w-7 shrink-0" />
              {/* Provider name + region */}
              <div className="w-32 shrink-0 space-y-1">
                <Skeleton variant="text" className="h-4 w-28" />
                <Skeleton variant="text" className="h-3 w-16" />
              </div>
              {/* Status */}
              <div className="flex items-center gap-1.5 w-20 shrink-0">
                <Skeleton variant="circle" className="h-2 w-2" />
                <Skeleton variant="text" className="h-3 w-12" />
              </div>
              {/* Latency */}
              <Skeleton variant="text" className="h-4 w-14 shrink-0" />
              {/* Tokens */}
              <Skeleton variant="text" className="h-4 w-16 shrink-0" />
              {/* Requests */}
              <Skeleton variant="text" className="h-4 w-16 shrink-0" />
              {/* PoNW Score */}
              <Skeleton variant="rect" className="h-5 w-12 rounded-full shrink-0" />
            </div>
          ))}
        </div>
      </div>

      {/* Mobile card skeleton (visible on small screens) */}
      <div className="md:hidden space-y-3">
        {Array.from({ length: 10 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-4"
          >
            <div className="flex items-center gap-3 mb-3">
              <Skeleton variant="circle" className="h-7 w-7 shrink-0" />
              <div className="flex-1 space-y-1">
                <Skeleton variant="text" className="h-4 w-32" />
                <Skeleton variant="text" className="h-3 w-16" />
              </div>
            </div>
            <div className="grid grid-cols-3 gap-3 pt-3 border-t border-surface-100">
              <div>
                <Skeleton variant="text" className="h-3 w-12 mb-1" />
                <Skeleton variant="text" className="h-4 w-14" />
              </div>
              <div>
                <Skeleton variant="text" className="h-3 w-12 mb-1" />
                <Skeleton variant="text" className="h-4 w-16" />
              </div>
              <div>
                <Skeleton variant="text" className="h-3 w-12 mb-1" />
                <Skeleton variant="text" className="h-4 w-14" />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default LeaderboardSkeleton;
