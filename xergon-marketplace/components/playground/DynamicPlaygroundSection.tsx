"use client";

import dynamic from "next/dynamic";

/**
 * Dynamically imported PlaygroundSection.
 * Uses next/dynamic with ssr:false to avoid SSR for this heavy
 * client-side component (chat interface, stores, etc.).
 * The parent page wraps this in <Suspense> for streaming.
 */
export const DynamicPlaygroundSection = dynamic(
  () =>
    import("@/components/playground/PlaygroundSection").then(
      (mod) => mod.PlaygroundSection,
    ),
  {
    ssr: false,
    loading: () => (
      <div className="mx-auto max-w-7xl px-4 py-16 md:py-24 text-center text-surface-400">
        Loading playground...
      </div>
    ),
  },
);
