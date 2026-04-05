export type Locale = "en" | "ja" | "zh" | "es";

export const DEFAULT_LOCALE: Locale = "en";

export interface LocaleInfo {
  code: Locale;
  name: string;
  flag: string;
  dir: "ltr" | "rtl";
}

export const SUPPORTED_LOCALES: LocaleInfo[] = [
  { code: "en", name: "English", flag: "\u{1F1FA}\u{1F1F8}", dir: "ltr" },
  { code: "ja", name: "\u65E5\u672C\u8A9E", flag: "\u{1F1EF}\u{1F1F5}", dir: "ltr" },
  { code: "zh", name: "\u7B80\u4F53\u4E2D\u6587", flag: "\u{1F1E8}\u{1F1F3}", dir: "ltr" },
  { code: "es", name: "Espa\u00F1ol", flag: "\u{1F1EA}\u{1F1F8}", dir: "ltr" },
];

/** Map from locale code to its display info */
export const LOCALE_MAP: Record<Locale, LocaleInfo> = Object.fromEntries(
  SUPPORTED_LOCALES.map((l) => [l.code, l])
) as Record<Locale, LocaleInfo>;

/** Detect locale from browser language setting */
export function getLocaleFromBrowser(): Locale {
  if (typeof navigator === "undefined") return DEFAULT_LOCALE;

  const lang = navigator.language.toLowerCase(); // e.g. "en-us", "ja", "zh-cn"

  // Exact match
  if (lang === "en" || lang.startsWith("en-")) return "en";
  if (lang === "ja" || lang.startsWith("ja-")) return "ja";
  if (lang === "zh" || lang.startsWith("zh-")) return "zh";
  if (lang === "es" || lang.startsWith("es-")) return "es";

  return DEFAULT_LOCALE;
}

/** Read persisted locale from localStorage */
export function getLocaleFromStorage(): Locale | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = localStorage.getItem("xergon-locale");
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    const code = parsed?.state?.locale;
    if (code && ["en", "ja", "zh", "es"].includes(code)) return code as Locale;
    return null;
  } catch {
    return null;
  }
}
