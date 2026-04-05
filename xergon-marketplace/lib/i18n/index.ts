"use client";

import { useCallback, useMemo } from "react";
import { useLocaleStore } from "@/lib/stores/locale";
import { dictionary } from "./dictionary";
import type { Locale } from "./config";

/**
 * Simple translation function.
 *
 * - Looks up `key` in the current locale's dictionary.
 * - Falls back to English if the key is missing in the current locale.
 * - Falls back to the key itself if missing in both locales.
 * - Supports interpolation: t("greeting", { name: "World" }) with "Hello {name}"
 */
export function translate(
  locale: Locale,
  key: string,
  params?: Record<string, string | number>,
): string {
  const value =
    dictionary[locale]?.[key] ?? dictionary.en?.[key] ?? key;

  if (!params) return value;

  return value.replace(/\{(\w+)\}/g, (match, paramKey) => {
    const replacement = params[paramKey];
    return replacement !== undefined ? String(replacement) : match;
  });
}

/**
 * React hook for translations.
 *
 * Usage:
 *   const { t, locale, setLocale } = useTranslation();
 *   <h1>{t("analytics.title")}</h1>
 *   <p>{t("explorer.providersCount", { count: 42 })}</p>
 */
export function useTranslation() {
  const locale = useLocaleStore((s) => s.locale);
  const setLocale = useLocaleStore((s) => s.setLocale);

  const t = useCallback(
    (key: string, params?: Record<string, string | number>): string => {
      return translate(locale, key, params);
    },
    [locale],
  );

  // Stable reference – re-memo only when locale changes
  const memoizedT = useMemo(() => t, [t]);

  return { t: memoizedT, locale, setLocale } as const;
}
