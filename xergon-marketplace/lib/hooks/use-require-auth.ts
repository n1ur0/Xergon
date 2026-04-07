"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuthStore } from "@/lib/stores/auth";

/**
 * useRequireAuth -- page-level auth guard hook.
 *
 * Returns the auth state and, when not authenticated on an operator page,
 * redirects the user to the home page with ?auth=required.
 *
 * Usage in page components:
 *   const { isAuthenticated, isLoading } = useRequireAuth();
 *   if (isLoading) return <LoadingSpinner />;
 *   if (!isAuthenticated) return null; // redirect in flight
 *   // render page ...
 */

export function useRequireAuth() {
  const router = useRouter();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoading = useAuthStore((s) => s.isLoading);

  useEffect(() => {
    // Only redirect after initial auth check completes
    if (!isLoading && !isAuthenticated) {
      router.replace("/?auth=required");
    }
  }, [isLoading, isAuthenticated, router]);

  return { isAuthenticated, isLoading };
}

export default useRequireAuth;
