"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";

/**
 * Client wrapper that checks onboarding status.
 * If the user has already completed onboarding, redirect to dashboard.
 */
export function OnboardingGuard({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const completed = localStorage.getItem("xergon_onboarding_completed");
    if (completed) {
      router.replace("/dashboard");
      return;
    }
    setReady(true);
  }, [router]);

  if (!ready) {
    return (
      <div className="flex min-h-dvh items-center justify-center bg-surface-50 dark:bg-surface-950">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-surface-300 border-t-emerald-500" />
      </div>
    );
  }

  return <>{children}</>;
}
