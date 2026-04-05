"use client";

import { useEffect } from "react";
import { Navbar } from "@/components/Navbar";
import { SkipToContent } from "@/components/a11y/SkipToContent";
import { useAuthStore } from "@/lib/stores/auth";

export function AppShell({ children }: { children: React.ReactNode }) {
  const restore = useAuthStore((s) => s.restore);

  useEffect(() => {
    restore();
  }, [restore]);

  return (
    <div className="flex min-h-[100dvh] flex-col">
      <SkipToContent />
      <Navbar />
      <main id="main-content" className="flex-1 overflow-x-hidden">
        {children}
      </main>
    </div>
  );
}
