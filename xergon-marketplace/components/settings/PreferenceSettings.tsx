"use client";

import { useState, useEffect, useCallback } from "react";
import { useThemeStore, type Theme } from "@/lib/stores/theme";
import { useLocaleStore } from "@/lib/stores/locale";
import { SUPPORTED_LOCALES, type Locale } from "@/lib/i18n/config";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface UserPreferences {
  defaultModel: string;
  outputFormat: "text" | "json" | "markdown";
}

const POPULAR_MODELS = [
  "llama-3.1-70b",
  "llama-3.1-8b",
  "mistral-7b",
  "qwen-2.5-72b",
  "deepseek-v3",
  "phi-4",
];

const OUTPUT_FORMATS: Array<{ value: UserPreferences["outputFormat"]; label: string }> = [
  { value: "text", label: "Plain Text" },
  { value: "json", label: "JSON" },
  { value: "markdown", label: "Markdown" },
];

const THEME_OPTIONS: Array<{ value: Theme; label: string; icon: string }> = [
  { value: "light", label: "Light", icon: "☀️" },
  { value: "dark", label: "Dark", icon: "🌙" },
  { value: "system", label: "System", icon: "💻" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function PreferenceSettings() {
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);
  const locale = useLocaleStore((s) => s.locale);
  const setLocale = useLocaleStore((s) => s.setLocale);

  const [defaultModel, setDefaultModel] = useState("llama-3.1-70b");
  const [outputFormat, setOutputFormat] = useState<UserPreferences["outputFormat"]>("text");
  const [saving, setSaving] = useState(false);

  // Load saved preferences
  useEffect(() => {
    try {
      const saved = localStorage.getItem("xergon-preferences");
      if (saved) {
        const data = JSON.parse(saved);
        setDefaultModel(data.defaultModel || "llama-3.1-70b");
        setOutputFormat(data.outputFormat || "text");
      }
    } catch {
      // use defaults
    }
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    await new Promise((r) => setTimeout(r, 500));
    localStorage.setItem("xergon-preferences", JSON.stringify({
      defaultModel,
      outputFormat,
    }));
    setSaving(false);
    toast.success("Preferences saved");
  }, [defaultModel, outputFormat]);

  return (
    <div className="space-y-6">
      {/* Theme */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Theme</h2>
        <p className="text-sm text-surface-800/50 mb-4">Choose how Xergon looks to you</p>
        <div className="flex items-center gap-3">
          {THEME_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setTheme(opt.value)}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border text-sm font-medium transition-colors ${
                theme === opt.value
                  ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                  : "border-surface-200 text-surface-800/60 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
              }`}
            >
              <span className="text-base">{opt.icon}</span>
              {opt.label}
            </button>
          ))}
        </div>
      </section>

      {/* Language */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Language</h2>
        <p className="text-sm text-surface-800/50 mb-4">Select your preferred language</p>
        <div className="flex flex-wrap items-center gap-2">
          {SUPPORTED_LOCALES.map((loc) => (
            <button
              key={loc.code}
              onClick={() => setLocale(loc.code as Locale)}
              className={`flex items-center gap-2 px-4 py-2.5 rounded-lg border text-sm font-medium transition-colors ${
                locale === loc.code
                  ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                  : "border-surface-200 text-surface-800/60 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
              }`}
            >
              <span className="text-base">{loc.flag}</span>
              {loc.name}
            </button>
          ))}
        </div>
      </section>

      {/* Default model */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Default Model</h2>
        <p className="text-sm text-surface-800/50 mb-4">Model to use when starting a new conversation</p>
        <select
          value={defaultModel}
          onChange={(e) => setDefaultModel(e.target.value)}
          className="w-full sm:w-72 px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          {POPULAR_MODELS.map((model) => (
            <option key={model} value={model}>{model}</option>
          ))}
        </select>
      </section>

      {/* Output format */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Output Format</h2>
        <p className="text-sm text-surface-800/50 mb-4">Default format for model responses</p>
        <div className="flex flex-wrap items-center gap-2">
          {OUTPUT_FORMATS.map((fmt) => (
            <button
              key={fmt.value}
              onClick={() => setOutputFormat(fmt.value)}
              className={`px-4 py-2.5 rounded-lg border text-sm font-medium transition-colors ${
                outputFormat === fmt.value
                  ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                  : "border-surface-200 text-surface-800/60 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
              }`}
            >
              {fmt.label}
            </button>
          ))}
        </div>
      </section>

      {/* Save */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSave}
          disabled={saving}
          className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50"
        >
          {saving ? "Saving..." : "Save Preferences"}
        </button>
      </div>
    </div>
  );
}
