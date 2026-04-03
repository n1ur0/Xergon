"use client";

import { useAuthStore } from "@/lib/stores/auth";

export function CreditsBadge() {
  const user = useAuthStore((s) => s.user);

  if (!user) {
    return (
      <span className="text-sm text-surface-800/40">No account</span>
    );
  }

  const usd = user.credits.toFixed(2);

  return (
    <div className="flex items-center gap-1.5 text-sm">
      <span className="inline-block h-2 w-2 rounded-full bg-accent-500" />
      <span className="font-medium text-surface-900">${usd}</span>
      <span className="text-surface-800/40">credits</span>
    </div>
  );
}
