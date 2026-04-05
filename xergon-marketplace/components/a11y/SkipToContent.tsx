/**
 * SkipToContent - "Skip to main content" link for keyboard navigation.
 *
 * Visible only when focused (via keyboard Tab). Clicking jumps focus to
 * the main content area.
 */

"use client";

import { useEffect, useRef } from "react";

export function SkipToContent() {
  const mainId = "main-content";

  return (
    <a
      href={`#${mainId}`}
      className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-[200] focus:rounded-lg focus:bg-brand-600 focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:text-white focus:shadow-lg focus:outline-none focus:ring-2 focus:ring-brand-500 focus:ring-offset-2"
    >
      Skip to main content
    </a>
  );
}
