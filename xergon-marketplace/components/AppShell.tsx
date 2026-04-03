"use client";

import { useEffect } from "react";
import { Navbar } from "@/components/Navbar";
import { useAuthStore } from "@/lib/stores/auth";

export function AppShell({ children }: { children: React.ReactNode }) {
  const restore = useAuthStore((s) => s.restore);

  useEffect(() => {
    restore();
  }, [restore]);

  return (
    <>
      <Navbar />
      <main className="flex-1">{children}</main>
    </>
  );
}
