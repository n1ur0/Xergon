"use client";

import { Sun, Moon, Monitor } from "lucide-react";
import { useThemeStore, type Theme, type ResolvedTheme } from "@/lib/stores/theme";

const LABELS: Record<Theme, string> = {
  light: "Light theme",
  dark: "Dark theme",
  system: "System theme",
};

const RESOLVED_LABELS: Record<ResolvedTheme, string> = {
  light: "Light",
  dark: "Dark",
};

function ThemeIcon({ theme, resolved }: { theme: Theme; resolved: ResolvedTheme }) {
  if (theme === "system") {
    return <Monitor className="h-4 w-4" />;
  }
  if (resolved === "dark") {
    return <Moon className="h-4 w-4" />;
  }
  return <Sun className="h-4 w-4" />;
}

export function ThemeToggle() {
  const theme = useThemeStore((s) => s.theme);
  const resolvedTheme = useThemeStore((s) => s.resolvedTheme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  const label = `${LABELS[theme]} (${RESOLVED_LABELS[resolvedTheme]}) — click to switch`;

  return (
    <button
      type="button"
      onClick={toggleTheme}
      aria-label={label}
      title={label}
      className="inline-flex items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
    >
      <ThemeIcon theme={theme} resolved={resolvedTheme} />
    </button>
  );
}
