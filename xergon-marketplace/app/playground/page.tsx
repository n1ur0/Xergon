"use client";

import { Suspense } from "react";
import { PlaygroundPage } from "@/components/playground/PlaygroundPage";

export default function Page() {
  return (
    <Suspense fallback={<div className="flex h-[calc(100vh-3.5rem)] items-center justify-center text-surface-800/40">Loading...</div>}>
      <PlaygroundPage />
    </Suspense>
  );
}
