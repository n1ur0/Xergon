'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── HealthSkeleton ──
// Matches health page: status banner + 6 service cards + provider distribution + chart area

export function HealthSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <Skeleton variant="text" className="h-7 w-44" />
        <Skeleton variant="text" className="h-4 w-64 mt-1" />
      </div>

      {/* Status banner */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 px-5 py-4 flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Skeleton variant="circle" className="h-3 w-3" />
          <Skeleton variant="text" className="h-5 w-48" />
        </div>
        <Skeleton variant="text" className="h-4 w-20" />
      </div>

      {/* 6 service cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 mb-6">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-5"
          >
            <div className="flex items-center justify-between mb-3">
              <Skeleton variant="text" className="h-4 w-28" />
              <Skeleton variant="rect" className="h-5 w-20 rounded-full" />
            </div>
            <div className="space-y-2">
              <Skeleton variant="text" className="h-4 w-full" />
              <Skeleton variant="text" className="h-4 w-3/4" />
              <Skeleton variant="text" className="h-4 w-1/2" />
            </div>
          </div>
        ))}
      </div>

      {/* Chain info strip */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
        <Skeleton variant="text" className="h-5 w-28 mb-3" />
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i}>
              <Skeleton variant="text" className="h-3 w-24 mb-1" />
              <Skeleton variant="text" className="h-4 w-16" />
            </div>
          ))}
        </div>
      </div>

      {/* Provider distribution + Incidents area */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        {/* Distribution */}
        <div className="lg:col-span-1">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <Skeleton variant="text" className="h-5 w-36 mb-4 mx-auto" />
            <Skeleton variant="circle" className="h-28 w-28 mx-auto" />
            <div className="flex justify-center gap-4 mt-4">
              <Skeleton variant="text" className="h-4 w-16" />
              <Skeleton variant="text" className="h-4 w-16" />
              <Skeleton variant="text" className="h-4 w-16" />
            </div>
          </div>
        </div>

        {/* Incidents */}
        <div className="lg:col-span-2">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <Skeleton variant="text" className="h-5 w-36 mb-3" />
            <div className="space-y-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="flex items-start gap-3 p-3 rounded-lg border border-surface-100">
                  <Skeleton variant="rect" className="h-5 w-16 rounded-full shrink-0 mt-0.5" />
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <Skeleton variant="text" className="h-4 w-40" />
                      <Skeleton variant="text" className="h-3 w-16" />
                    </div>
                    <Skeleton variant="text" className="h-3 w-full" />
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Uptime bars chart area */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <Skeleton variant="text" className="h-5 w-36 mb-4" />
        <div className="space-y-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex items-center gap-3">
              <Skeleton variant="text" className="h-4 w-24 shrink-0" />
              <Skeleton variant="rect" className="h-4 w-full rounded-full" />
              <Skeleton variant="text" className="h-4 w-12 shrink-0" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default HealthSkeleton;
