"use client";

import { useAuthStore } from "@/lib/stores/auth";
import { ErgoAuthButton } from "@/components/auth/ErgoAuthButton";
import Link from "next/link";

/**
 * OperatorAuthGuard -- wraps operator page content.
 *
 * States:
 *  1. Loading  -> full-page skeleton spinner
 *  2. Not authenticated -> connect wallet prompt
 *  3. Authenticated     -> render children
 *
 * We intentionally do NOT gate on "operator role" yet because the
 * relay does not expose a role field. Once the relay API adds a role
 * check this component can easily be extended.
 */

export function OperatorAuthGuard({ children }: { children: React.ReactNode }) {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoading = useAuthStore((s) => s.isLoading);
  const user = useAuthStore((s) => s.user);

  // ---- Loading state ----
  if (isLoading) {
    return (
      <div className="flex items-center justify-center min-h-[60vh]">
        <div className="text-center space-y-4">
          <span className="inline-block h-8 w-8 animate-spin rounded-full border-3 border-brand-200 border-t-brand-600" />
          <p className="text-sm text-surface-800/50">Checking authentication...</p>
        </div>
      </div>
    );
  }

  // ---- Not authenticated ----
  if (!isAuthenticated) {
    return (
      <div className="flex items-center justify-center min-h-[60vh]">
        <div className="max-w-md w-full mx-auto rounded-2xl border border-surface-200 bg-surface-0 p-8 text-center space-y-6 shadow-sm">
          {/* Lock icon */}
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-full bg-brand-50">
            <svg
              className="h-7 w-7 text-brand-600"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </svg>
          </div>

          <div className="space-y-2">
            <h2 className="text-lg font-semibold text-surface-900">
              Operator Access Required
            </h2>
            <p className="text-sm text-surface-800/60 leading-relaxed">
              Connect your Ergo wallet to access the operator panel. Your
              wallet address is used to verify your identity and permissions.
            </p>
          </div>

          <ErgoAuthButton
            showErgoAuthFallback
            className="mx-auto"
            onSuccess={() => {
              // The store update will re-render this component automatically
            }}
          />

          <div className="pt-2 border-t border-surface-100">
            <p className="text-xs text-surface-800/40">
              New operator?{" "}
              <Link
                href="/onboarding"
                className="text-brand-600 hover:text-brand-700 font-medium underline underline-offset-2"
              >
                Start the onboarding flow
              </Link>
            </p>
          </div>
        </div>
      </div>
    );
  }

  // ---- Authenticated: render children ----
  return <>{children}</>;
}

export default OperatorAuthGuard;
