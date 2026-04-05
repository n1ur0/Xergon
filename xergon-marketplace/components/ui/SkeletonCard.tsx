"use client";

import { cn } from "@/lib/utils";

interface SkeletonCardProps {
  className?: string;
}

/**
 * Skeleton placeholder matching the GpuCard / ModelCard layout.
 * Uses shimmer animation instead of plain pulse.
 */
export function SkeletonCard({ className }: SkeletonCardProps) {
  return (
    <div
      className={cn(
        "rounded-xl border border-surface-200 bg-surface-0 p-5",
        className,
      )}
    >
      {/* Header */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex-1">
          <div className="h-5 w-2/3 rounded skeleton-shimmer mb-2" />
          <div className="h-3 w-24 rounded skeleton-shimmer" />
        </div>
        <div className="h-5 w-14 rounded-full skeleton-shimmer" />
      </div>

      {/* Specs grid */}
      <div className="grid grid-cols-2 gap-2 mb-4">
        <div className="h-12 rounded-lg skeleton-shimmer" />
        <div className="h-12 rounded-lg skeleton-shimmer" />
      </div>

      {/* Rating placeholder */}
      <div className="h-3 w-20 rounded skeleton-shimmer mb-3" />

      {/* Footer */}
      <div className="flex items-center justify-between pt-3 border-t border-surface-100">
        <div className="h-4 w-24 rounded skeleton-shimmer" />
        <div className="h-8 w-16 rounded-lg skeleton-shimmer" />
      </div>
    </div>
  );
}

interface SkeletonCardGridProps {
  count?: number;
  className?: string;
}

export function SkeletonCardGrid({ count = 6, className }: SkeletonCardGridProps) {
  return (
    <div className={cn("grid gap-4 sm:grid-cols-2 lg:grid-cols-3", className)}>
      {Array.from({ length: count }).map((_, i) => (
        <SkeletonCard key={i} />
      ))}
    </div>
  );
}
