"use client";

import { useMemo } from "react";
import { useLocaleStore } from "@/lib/stores/locale";
import { dictionary } from "@/lib/i18n/dictionary";
import { DEFAULT_LOCALE } from "@/lib/i18n/config";

/**
 * Translation hook.
 *
 * Usage:
 *   const t = useT();
 *   t("nav.home")  => "Home"
 *   t("greeting", { name: "Alice" })  => "Hello, Alice"
 *
 * Falls back to English, then to the raw key (for debugging).
 */
export function useT() {
  const locale = useLocaleStore((s) => s.locale);

  const t = useMemo(() => {
    return (key: string, params?: Record<string, string | number>): string => {
      // Try current locale first
      let text = dictionary[locale]?.[key];

      // Fallback to English
      if (text === undefined) {
        text = dictionary[DEFAULT_LOCALE]?.[key];
      }

      // Ultimate fallback: return key itself so missing translations are visible
      if (text === undefined) {
        return key;
      }

      // Interpolate parameters: {name}, {count}, etc.
      if (params) {
        for (const [k, v] of Object.entries(params)) {
          text = text.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
        }
      }

      return text;
    };
  }, [locale]);

  return t;
}
