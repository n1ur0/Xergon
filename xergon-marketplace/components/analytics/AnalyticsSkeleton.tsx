'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── AnalyticsSkeleton ──
// Matches analytics page: 6 metric cards + chart area + table skeleton

export function AnalyticsSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div className="space-y-1">
          <Skeleton variant="text" className="h-7 w-48" />
          <Skeleton variant="text" className="h-4 w-72" />
        </div>
        <Skeleton variant="rect" className="h-8 w-24 rounded-full" />
      </div>

      {/* 6 metric stat cards (matching StatsHero layout) */}
      <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4 mb-6">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-5"
          >
            <Skeleton variant="rect" className="h-8 w-8 rounded-lg mb-3" />
            <Skeleton variant="text" className="h-7 w-24 mb-1.5" />
            <Skeleton variant="text" className="h-4 w-16 mb-1" />
            <Skeleton variant="text" className="h-3 w-12" />
          </div>
        ))}
      </div>

      {/* Chart area + sidebar */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        {/* Chart (2/3) */}
        <div className="lg:col-span-2">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <div className="flex items-center justify-between mb-4">
              <Skeleton variant="text" className="h-5 w-32" />
              <Skeleton variant="rect" className="h-7 w-20 rounded-lg" />
            </div>
            <Skeleton variant="rect" className="h-[260px] w-full" />
          </div>
        </div>

        {/* Sidebar: uptime + regions */}
        <div className="space-y-6">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <Skeleton variant="text" className="h-5 w-28 mb-3 mx-auto" />
            <Skeleton variant="circle" className="h-24 w-24 mx-auto" />
          </div>
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <Skeleton variant="text" className="h-5 w-36 mb-3" />
            <div className="space-y-3">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i}>
                  <Skeleton variant="text" className="h-4 w-20 mb-1" />
                  <Skeleton variant="rect" className="h-2 w-full rounded-full" />
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-100">
          <Skeleton variant="text" className="h-5 w-24 mb-1" />
          <Skeleton variant="text" className="h-3 w-40" />
        </div>
        <div className="space-y-0">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 px-5 py-3 border-b border-surface-50">
              <Skeleton variant="rect" className="h-4 w-4" />
              <Skeleton variant="text" className="h-4 w-32" />
              <div className="flex-1" />
              <Skeleton variant="text" className="h-4 w-16" />
              <Skeleton variant="text" className="h-4 w-16" />
              <Skeleton variant="rect" className="h-1.5 w-24 rounded-full" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default AnalyticsSkeleton;
