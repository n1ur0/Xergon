'use client';

import React, { Suspense, type ReactNode } from 'react';
import { PageSkeleton } from '@/components/ui/PageSkeleton';

// ── SuspenseWrap ──
// Simple wrapper that places children inside React.Suspense with a fallback.

interface SuspenseWrapProps {
  children: ReactNode;
  /** Optional custom fallback — defaults to PageSkeleton */
  fallback?: ReactNode;
}

export function SuspenseWrap({ children, fallback }: SuspenseWrapProps) {
  return (
    <Suspense fallback={fallback ?? <PageSkeleton />}>
      {children}
    </Suspense>
  );
}

export default SuspenseWrap;
