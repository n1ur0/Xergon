import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Locale } from "@/lib/i18n/config";
import { DEFAULT_LOCALE, getLocaleFromBrowser } from "@/lib/i18n/config";

interface LocaleState {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  /** Read browser language and persist it; only has effect if no locale is stored yet */
  detectAndSetLocale: () => void;
}

function applyLocale(locale: Locale) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
}

export const useLocaleStore = create<LocaleState>()(
  persist(
    (set, get) => ({
      locale: DEFAULT_LOCALE,

      setLocale: (locale: Locale) => {
        applyLocale(locale);
        set({ locale });
      },

      detectAndSetLocale: () => {
        // Only auto-detect if still on default (nothing stored or explicitly set)
        const current = get().locale;
        if (current !== DEFAULT_LOCALE) {
          // Already has a user preference, just re-apply
          applyLocale(current);
          return;
        }
        const detected = getLocaleFromBrowser();
        applyLocale(detected);
        set({ locale: detected });
      },
    }),
    {
      name: "xergon-locale",
      partialize: (state) => ({ locale: state.locale }),
      onRehydrateStorage: () => (state) => {
        if (state) {
          applyLocale(state.locale);
        }
      },
    }
  )
);
