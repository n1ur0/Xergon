"use client";

import { useEffect, useRef } from "react";
import { useThemeStore } from "@/lib/stores/theme";

/**
 * Applies the resolved theme class ("dark" / "light") to <html> whenever
 * the theme store changes.  Must be rendered as a client component inside
 * the React tree so it stays in sync with the Zustand store.
 */
export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const theme = useThemeStore((s) => s.theme);
  const resolvedTheme = useThemeStore((s) => s.resolvedTheme);
  const prevResolved = useRef(resolvedTheme);

  useEffect(() => {
    if (prevResolved.current !== resolvedTheme) {
      const root = document.documentElement;
      root.classList.remove("light", "dark");
      root.classList.add(resolvedTheme);
      prevResolved.current = resolvedTheme;
    }
  }, [resolvedTheme]);

  // Also react to explicit theme changes (e.g. system -> light) that may
  // not change resolvedTheme but still need a class refresh.
  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(resolvedTheme);
  }, [theme, resolvedTheme]);

  return <>{children}</>;
}
