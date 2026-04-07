"use client";

import { Settings, Sun, Moon, Monitor, Bell, Mail, Send, Globe } from "lucide-react";
import type { Theme } from "@/lib/stores/theme";
import type { Locale } from "@/lib/i18n/config";
import { SUPPORTED_LOCALES } from "@/lib/i18n/config";

interface PreferencesStepProps {
  value: {
    defaultModel: string;
    notifications: {
      email: boolean;
      push: boolean;
      telegram: boolean;
    };
    theme: Theme;
    language: Locale;
    privacyProfile: boolean;
    privacyActivity: boolean;
  };
  onChange: (update: Partial<PreferencesStepProps["value"]>) => void;
}

const POPULAR_MODELS = [
  "Auto (best available)",
  "llama-3.3-70b",
  "mistral-small-24b",
  "llama-3.1-8b",
  "qwen3.5-4b-f16.gguf",
];

const THEMES: { value: Theme; label: string; icon: React.ReactNode }[] = [
  { value: "light", label: "Light", icon: <Sun className="h-4 w-4" /> },
  { value: "dark", label: "Dark", icon: <Moon className="h-4 w-4" /> },
  { value: "system", label: "System", icon: <Monitor className="h-4 w-4" /> },
];

export default function PreferencesStep({ value, onChange }: PreferencesStepProps) {
  return (
    <div className="space-y-6">
      <div className="text-center space-y-3">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-cyan-500 to-blue-600 shadow-lg shadow-cyan-500/20">
          <Settings className="h-8 w-8 text-white" />
        </div>
        <h2 className="text-xl font-bold text-surface-900 dark:text-surface-0">
          Preferences
        </h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60 max-w-md mx-auto">
          Customize your Xergon experience. These can be changed anytime in settings.
        </p>
      </div>

      <div className="mx-auto max-w-lg space-y-5">
        {/* Default Model */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            Default Model
          </label>
          <select
            value={value.defaultModel}
            onChange={(e) => onChange({ defaultModel: e.target.value })}
            className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2.5 text-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900"
          >
            {POPULAR_MODELS.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>

        {/* Notifications */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-3">
            Notifications
          </label>
          <div className="space-y-3">
            <ToggleRow
              icon={<Mail className="h-4 w-4" />}
              label="Email notifications"
              description="Receive updates via email"
              checked={value.notifications.email}
              onChange={(checked) =>
                onChange({ notifications: { ...value.notifications, email: checked } })
              }
            />
            <ToggleRow
              icon={<Bell className="h-4 w-4" />}
              label="Push notifications"
              description="Browser push notifications"
              checked={value.notifications.push}
              onChange={(checked) =>
                onChange({ notifications: { ...value.notifications, push: checked } })
              }
            />
            <ToggleRow
              icon={<Send className="h-4 w-4" />}
              label="Telegram alerts"
              description="Get alerts via Telegram bot"
              checked={value.notifications.telegram}
              onChange={(checked) =>
                onChange({ notifications: { ...value.notifications, telegram: checked } })
              }
            />
          </div>
        </div>

        {/* Theme */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-2">
            Theme
          </label>
          <div className="grid grid-cols-3 gap-2">
            {THEMES.map((t) => (
              <button
                key={t.value}
                type="button"
                onClick={() => onChange({ theme: t.value })}
                className={`flex items-center justify-center gap-2 rounded-lg border-2 py-2.5 text-sm font-medium transition-all ${
                  value.theme === t.value
                    ? "border-emerald-500 bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300"
                    : "border-surface-200 text-surface-600 hover:border-surface-300 dark:border-surface-700 dark:text-surface-400 dark:hover:border-surface-600"
                }`}
              >
                {t.icon}
                {t.label}
              </button>
            ))}
          </div>
        </div>

        {/* Language */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-1.5">
            <Globe className="inline h-4 w-4 mr-1" />
            Language
          </label>
          <select
            value={value.language}
            onChange={(e) => onChange({ language: e.target.value as Locale })}
            className="block w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2.5 text-sm focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-surface-600 dark:bg-surface-900"
          >
            {SUPPORTED_LOCALES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.flag} {l.name}
              </option>
            ))}
          </select>
        </div>

        {/* Privacy */}
        <div>
          <label className="block text-sm font-medium text-surface-700 dark:text-surface-300 mb-3">
            Privacy
          </label>
          <div className="space-y-3">
            <ToggleRow
              icon={null}
              label="Public profile"
              description="Show your profile in the provider directory"
              checked={value.privacyProfile}
              onChange={(checked) => onChange({ privacyProfile: checked })}
            />
            <ToggleRow
              icon={null}
              label="Activity visible"
              description="Allow others to see your recent activity"
              checked={value.privacyActivity}
              onChange={(checked) => onChange({ privacyActivity: checked })}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── Toggle row sub-component ── */

function ToggleRow({
  icon,
  label,
  description,
  checked,
  onChange,
}: {
  icon: React.ReactNode;
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-surface-200 bg-surface-0 px-4 py-3 dark:border-surface-700 dark:bg-surface-900">
      <div className="flex items-center gap-3">
        {icon && <span className="text-surface-400">{icon}</span>}
        <div>
          <p className="text-sm font-medium text-surface-900 dark:text-surface-0">
            {label}
          </p>
          <p className="text-xs text-surface-800/50 dark:text-surface-300/50">
            {description}
          </p>
        </div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:ring-offset-2 ${
          checked ? "bg-emerald-500" : "bg-surface-300 dark:bg-surface-600"
        }`}
      >
        <span
          className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow-sm ring-0 transition-transform ${
            checked ? "translate-x-5" : "translate-x-0"
          }`}
        />
      </button>
    </div>
  );
}
