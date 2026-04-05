'use client';

import { Skeleton } from '@/components/ui/Skeleton';

// ── PageSkeleton ──
// Full page loading skeleton that matches the app layout structure.

interface PageSkeletonProps {
  /** Number of content row placeholders (default 5) */
  rows?: number;
  /** Whether to show a sidebar skeleton */
  hasSidebar?: boolean;
}

export function PageSkeleton({ rows = 5, hasSidebar = false }: PageSkeletonProps) {
  return (
    <div className="mx-auto max-w-6xl px-4 py-8 animate-in fade-in duration-300">
      {/* Navbar area placeholder */}
      <div className="mb-8 flex items-center justify-between">
        <div className="space-y-2">
          <Skeleton variant="text" className="h-7 w-48" />
          <Skeleton variant="text" className="h-4 w-72" />
        </div>
        <Skeleton variant="rect" className="h-9 w-28 hidden sm:block" />
      </div>

      {/* Main content area */}
      <div className={hasSidebar ? 'grid grid-cols-1 lg:grid-cols-3 gap-6' : ''}>
        {/* Primary content */}
        <div className={hasSidebar ? 'lg:col-span-2 space-y-4' : 'space-y-4'}>
          {Array.from({ length: rows }).map((_, i) => (
            <Skeleton key={i} variant="card" className="h-20 w-full" />
          ))}
        </div>

        {/* Sidebar */}
        {hasSidebar && (
          <div className="space-y-4">
            <Skeleton variant="card" className="h-48 w-full" />
            <Skeleton variant="card" className="h-36 w-full" />
            <Skeleton variant="card" className="h-40 w-full" />
          </div>
        )}
      </div>
    </div>
  );
}

export default PageSkeleton;
