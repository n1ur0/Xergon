"use client";

import { useEffect } from "react";
import { useLocaleStore } from "@/lib/stores/locale";

/**
 * Calls detectAndSetLocale() once on mount so the zustand store
 * picks up the browser language when no persisted preference exists.
 */
export function LocaleInit() {
  const detectAndSetLocale = useLocaleStore((s) => s.detectAndSetLocale);

  useEffect(() => {
    detectAndSetLocale();
  }, [detectAndSetLocale]);

  return null;
}
