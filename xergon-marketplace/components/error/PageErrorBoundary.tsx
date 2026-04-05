'use client';

import { type ReactNode } from 'react';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { ErrorBoundary } from '@/components/error/ErrorBoundary';

// ── PageErrorBoundary ──
// Wrapper that catches errors for specific page sections.
// Uses brand colors instead of red-tinted UI.

interface PageErrorBoundaryProps {
  children: ReactNode;
  /** Optional section context shown in the fallback */
  context?: string;
  className?: string;
}

function PageFallback({ context, onRetry }: { context?: string; onRetry: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center rounded-xl border border-surface-200 bg-surface-0 px-6 py-12 text-center">
      <AlertCircle className="mb-4 h-10 w-10 text-brand-500" />
      <h3 className="mb-1 text-lg font-semibold text-surface-900">
        Something went wrong
      </h3>
      {context && (
        <p className="mb-2 text-sm text-surface-800/50">
          While loading: {context}
        </p>
      )}
      <p className="mb-6 max-w-md text-sm text-surface-800/60">
        An unexpected error occurred. Please try again.
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 focus:outline-none focus:ring-2 focus:ring-brand-500 focus:ring-offset-2"
      >
        <RefreshCw className="h-4 w-4" />
        Try Again
      </button>
    </div>
  );
}

export function PageErrorBoundary({ children, context, className }: PageErrorBoundaryProps) {
  return (
    <ErrorBoundary
      className={className}
      fallback={<PageFallback context={context} onRetry={() => window.location.reload()} />}
    >
      {children}
    </ErrorBoundary>
  );
}

export default PageErrorBoundary;
