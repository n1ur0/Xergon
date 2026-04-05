"use client";

import { Suspense } from "react";
import { PlaygroundPage } from "@/components/playground/PlaygroundPage";

function PlaygroundFallback() {
  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col items-center justify-center gap-3">
      <div className="h-8 w-48 rounded-lg skeleton-shimmer" />
      <div className="h-4 w-32 rounded skeleton-shimmer" />
      <div className="h-1 w-24 rounded skeleton-shimmer" />
    </div>
  );
}

export default function Page() {
  return (
    <Suspense fallback={<PlaygroundFallback />}>
      <PlaygroundPage />
    </Suspense>
  );
}
