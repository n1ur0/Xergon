'use client';

import { cn } from '@/lib/utils';

// ── Skeleton primitive ──

interface SkeletonProps {
  /** Additional CSS classes for sizing / layout */
  className?: string;
  /** Preset variant shapes */
  variant?: 'text' | 'circle' | 'card' | 'rect';
}

const variantClasses: Record<NonNullable<SkeletonProps['variant']>, string> = {
  text: 'h-4 w-3/4 rounded',
  circle: 'rounded-full',
  card: 'rounded-xl p-4',
  rect: 'rounded-lg',
};

export function Skeleton({ className, variant = 'rect' }: SkeletonProps) {
  return (
    <div
      className={cn('skeleton-shimmer bg-surface-200', variantClasses[variant], className)}
      aria-hidden="true"
    />
  );
}

export default Skeleton;
