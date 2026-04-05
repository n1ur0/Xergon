'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── TransactionsSkeleton ──
// Matches transactions page: header + summary cards + filters + table with 8 rows

export function TransactionsSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div className="space-y-1">
          <Skeleton variant="text" className="h-7 w-48" />
          <Skeleton variant="text" className="h-4 w-72" />
        </div>
        <div className="flex items-center gap-3">
          <Skeleton variant="rect" className="h-8 w-28 rounded-lg" />
          <Skeleton variant="rect" className="h-8 w-20 rounded-lg" />
          <Skeleton variant="rect" className="h-8 w-20 rounded-lg" />
        </div>
      </div>

      {/* Summary cards (4) */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        {Array.from({ length: 4 }).map((_, i) => (
          <div
            key={i}
            className="rounded-xl border border-surface-200 bg-surface-0 p-4"
          >
            <Skeleton variant="text" className="h-3 w-20 mb-2" />
            <Skeleton variant="text" className="h-6 w-24 mb-1" />
            <Skeleton variant="text" className="h-3 w-16" />
          </div>
        ))}
      </div>

      {/* Filters bar */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-3 mb-4">
        <div className="flex flex-wrap gap-2">
          <Skeleton variant="rect" className="h-8 w-20 rounded-lg" />
          <Skeleton variant="rect" className="h-8 w-20 rounded-lg" />
          <Skeleton variant="rect" className="h-8 w-24 rounded-lg" />
          <div className="flex-1" />
          <Skeleton variant="rect" className="h-8 w-24 rounded-lg" />
        </div>
      </div>

      {/* Table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        {/* Table header */}
        <div className="border-b border-surface-200 bg-surface-50 px-4 py-3">
          <div className="flex items-center gap-4">
            <Skeleton variant="text" className="h-3 w-16 uppercase" />
            <Skeleton variant="text" className="h-3 w-14 uppercase" />
            <Skeleton variant="text" className="h-3 w-16 uppercase" />
            <Skeleton variant="text" className="h-3 w-20 uppercase" />
            <Skeleton variant="text" className="h-3 w-14 uppercase" />
            <Skeleton variant="text" className="h-3 w-20 uppercase" />
          </div>
        </div>

        {/* Table rows */}
        <div className="divide-y divide-surface-100">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 px-4 py-3">
              {/* Type */}
              <Skeleton variant="rect" className="h-5 w-16 rounded-full shrink-0" />
              {/* Date */}
              <Skeleton variant="text" className="h-4 w-24 shrink-0" />
              {/* Amount */}
              <Skeleton variant="text" className="h-4 w-20 shrink-0" />
              {/* Status */}
              <Skeleton variant="rect" className="h-5 w-16 rounded-full shrink-0" />
              {/* Confirmations */}
              <Skeleton variant="text" className="h-4 w-20 shrink-0" />
              <div className="flex-1" />
              {/* Action */}
              <Skeleton variant="rect" className="h-4 w-16 shrink-0" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default TransactionsSkeleton;
