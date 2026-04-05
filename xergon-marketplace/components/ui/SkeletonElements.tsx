"use client";

import { cn } from "@/lib/utils";

interface SkeletonMetricProps {
  className?: string;
}

/** Skeleton for a MetricCard in the Provider Dashboard. */
export function SkeletonMetric({ className }: SkeletonMetricProps) {
  return (
    <div className={cn("rounded-xl border border-surface-200 bg-surface-0 p-4", className)}>
      <div className="h-3 w-20 rounded skeleton-shimmer mb-2" />
      <div className="h-7 w-24 rounded skeleton-shimmer mb-1" />
      <div className="h-3 w-32 rounded skeleton-shimmer" />
    </div>
  );
}

interface SkeletonMetricGridProps {
  count?: number;
  className?: string;
}

export function SkeletonMetricGrid({ count = 4, className }: SkeletonMetricGridProps) {
  return (
    <div className={cn("grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-8", className)}>
      {Array.from({ length: count }).map((_, i) => (
        <SkeletonMetric key={i} />
      ))}
    </div>
  );
}

/** Skeleton for a table row. */
export function SkeletonTableRow({ className }: { className?: string }) {
  return (
    <tr className={cn("border-b border-surface-50", className)}>
      <td className="py-2.5 px-5"><div className="h-4 w-24 rounded skeleton-shimmer" /></td>
      <td className="py-2.5 px-3 text-right"><div className="h-4 w-16 rounded skeleton-shimmer ml-auto" /></td>
      <td className="py-2.5 px-3 text-right"><div className="h-4 w-14 rounded skeleton-shimmer ml-auto" /></td>
      <td className="py-2.5 px-3 text-right"><div className="h-4 w-14 rounded skeleton-shimmer ml-auto" /></td>
      <td className="py-2.5 px-5 text-right"><div className="h-4 w-10 rounded skeleton-shimmer ml-auto" /></td>
    </tr>
  );
}

/** Skeleton for a pricing table. */
export function SkeletonPricingTable({ className }: { className?: string }) {
  return (
    <div className={cn("rounded-xl border border-surface-200 bg-surface-0 overflow-hidden", className)}>
      <div className="px-5 py-3 border-b border-surface-100">
        <div className="h-4 w-32 rounded skeleton-shimmer" />
      </div>
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100">
            <th className="px-5 py-2 font-medium">GPU Type</th>
            <th className="px-3 py-2 font-medium text-right">Avg Price</th>
            <th className="px-3 py-2 font-medium text-right">Min</th>
            <th className="px-3 py-2 font-medium text-right">Max</th>
            <th className="px-5 py-2 font-medium text-right">Listings</th>
          </tr>
        </thead>
        <tbody>
          {Array.from({ length: 5 }).map((_, i) => (
            <SkeletonTableRow key={i} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Skeleton for a rental list item. */
export function SkeletonRentalItem({ className }: { className?: string }) {
  return (
    <div className={cn("rounded-xl border border-surface-200 bg-surface-0 p-4", className)}>
      <div className="flex items-center gap-3">
        <div className="h-4 w-4 rounded skeleton-shimmer" />
        <div className="flex-1">
          <div className="h-4 w-32 rounded skeleton-shimmer mb-1" />
          <div className="h-3 w-48 rounded skeleton-shimmer" />
        </div>
        <div className="h-8 w-16 rounded-lg skeleton-shimmer" />
      </div>
    </div>
  );
}
