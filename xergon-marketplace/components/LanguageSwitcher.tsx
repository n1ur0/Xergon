"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import { useLocaleStore } from "@/lib/stores/locale";
import { SUPPORTED_LOCALES, LOCALE_MAP, type Locale } from "@/lib/i18n/config";

export function LanguageSwitcher({ compact = false }: { compact?: boolean }) {
  const locale = useLocaleStore((s) => s.locale);
  const setLocale = useLocaleStore((s) => s.setLocale);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const current = LOCALE_MAP[locale];

  // Close dropdown on outside click
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const handleSelect = useCallback(
    (code: Locale) => {
      setLocale(code);
      setOpen(false);
    },
    [setLocale]
  );

  // Compact mode: just a button showing the flag, clicking cycles through locales
  if (compact) {
    return (
      <button
        type="button"
        onClick={() => {
          const idx = SUPPORTED_LOCALES.findIndex((l) => l.code === locale);
          const next = SUPPORTED_LOCALES[(idx + 1) % SUPPORTED_LOCALES.length];
          handleSelect(next.code);
        }}
        aria-label={`Language: ${current.name} — click to change`}
        title={current.name}
        className="inline-flex items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
      >
        <span className="text-base leading-none">{current.flag}</span>
      </button>
    );
  }

  // Full dropdown mode
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-label={`Language: ${current.name} — click to change`}
        aria-expanded={open}
        aria-haspopup="listbox"
        className="inline-flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-sm text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors"
      >
        <span className="text-base leading-none">{current.flag}</span>
        <span className="hidden sm:inline">{current.name}</span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`transition-transform ${open ? "rotate-180" : ""}`}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      {/* Dropdown panel */}
      <div
        role="listbox"
        aria-label="Select language"
        className={`absolute right-0 top-full mt-1 min-w-[160px] rounded-lg border border-surface-200 bg-surface-0 shadow-lg py-1 z-50 transition-opacity duration-150 ${
          open
            ? "opacity-100 pointer-events-auto"
            : "opacity-0 pointer-events-none"
        }`}
      >
        {SUPPORTED_LOCALES.map((loc) => (
          <button
            key={loc.code}
            role="option"
            aria-selected={loc.code === locale}
            onClick={() => handleSelect(loc.code)}
            className={`flex w-full items-center gap-2 px-3 py-2 text-sm transition-colors ${
              loc.code === locale
                ? "bg-surface-100 text-surface-900 font-medium"
                : "text-surface-800/70 hover:bg-surface-50 hover:text-surface-900"
            }`}
          >
            <span className="text-base leading-none">{loc.flag}</span>
            <span>{loc.name}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
