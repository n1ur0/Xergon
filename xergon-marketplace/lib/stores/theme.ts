import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

interface ThemeState {
  theme: Theme;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

function getSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

function resolveTheme(theme: Theme): ResolvedTheme {
  if (theme === "system") return getSystemTheme();
  return theme;
}

function applyThemeClass(resolved: ResolvedTheme) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  root.classList.add(resolved);
}

export const useThemeStore = create<ThemeState>()(
  persist(
    (set, get) => ({
      theme: "system",
      resolvedTheme: "dark",

      setTheme: (theme: Theme) => {
        const resolved = resolveTheme(theme);
        applyThemeClass(resolved);
        set({ theme, resolvedTheme: resolved });
      },

      toggleTheme: () => {
        const { theme } = get();
        const next: Theme =
          theme === "light" ? "dark" : theme === "dark" ? "system" : "light";
        get().setTheme(next);
      },
    }),
    {
      name: "xergon-theme",
      partialize: (state) => ({ theme: state.theme }),
      onRehydrateStorage: () => (state) => {
        if (state) {
          const resolved = resolveTheme(state.theme);
          applyThemeClass(resolved);
          state.resolvedTheme = resolved;
        }
      },
    }
  )
);

// Listen for system preference changes
if (typeof window !== "undefined") {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", () => {
    const { theme, setTheme } = useThemeStore.getState();
    if (theme === "system") {
      const resolved = resolveTheme("system");
      applyThemeClass(resolved);
      useThemeStore.setState({ resolvedTheme: resolved });
    }
  });
}
