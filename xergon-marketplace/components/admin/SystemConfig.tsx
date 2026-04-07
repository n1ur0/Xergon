"use client";

import { useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FeatureToggle {
  key: string;
  label: string;
  description: string;
  enabled: boolean;
}

interface RateLimitConfig {
  requestsPerMinute: number;
  burstAllowance: number;
  rentalRequestsPerHour: number;
  apiCallsPerDay: number;
}

interface ConfigVersion {
  id: string;
  author: string;
  timestamp: string;
  description: string;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const INITIAL_FEATURES: FeatureToggle[] = [
  { key: "registration", label: "User Registration", description: "Allow new users to register via wallet connection", enabled: true },
  { key: "forum", label: "Forum", description: "Enable the community forum section", enabled: true },
  { key: "reviews", label: "Reviews", description: "Allow users to leave reviews on providers and models", enabled: true },
  { key: "chat_widget", label: "Chat Widget", description: "Show the embedded chat widget on pages", enabled: true },
  { key: "gpu_rentals", label: "GPU Rentals", description: "Enable GPU rental marketplace", enabled: true },
  { key: "model_marketplace", label: "Model Marketplace", description: "Enable the AI model listing and inference", enabled: true },
  { key: "leaderboard", label: "Leaderboard", description: "Show public provider leaderboard", enabled: true },
];

const INITIAL_RATE_LIMITS: RateLimitConfig = {
  requestsPerMinute: 60,
  burstAllowance: 10,
  rentalRequestsPerHour: 20,
  apiCallsPerDay: 10000,
};

const INITIAL_CONFIG_HISTORY: ConfigVersion[] = [
  { id: "v1", author: "admin", timestamp: "2026-04-05T10:00:00Z", description: "Initial configuration" },
  { id: "v2", author: "admin", timestamp: "2026-04-04T14:00:00Z", description: "Disabled forum for maintenance" },
  { id: "v3", author: "admin", timestamp: "2026-04-03T09:00:00Z", description: "Increased rate limits for premium tier" },
  { id: "v4", author: "admin", timestamp: "2026-04-02T16:00:00Z", description: "Enabled chat widget" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SystemConfig() {
  const [features, setFeatures] = useState<FeatureToggle[]>(INITIAL_FEATURES);
  const [rateLimits, setRateLimits] = useState<RateLimitConfig>(INITIAL_RATE_LIMITS);
  const [maintenanceMode, setMaintenanceMode] = useState(false);
  const [configHistory, setConfigHistory] = useState<ConfigVersion[]>(INITIAL_CONFIG_HISTORY);
  const [saving, setSaving] = useState(false);
  const [lastSaved, setLastSaved] = useState<string | null>(null);

  const toggleFeature = useCallback((key: string) => {
    setFeatures((prev) =>
      prev.map((f) => (f.key === key ? { ...f, enabled: !f.enabled } : f)),
    );
  }, []);

  const updateRateLimit = useCallback((key: keyof RateLimitConfig, value: number) => {
    setRateLimits((prev) => ({ ...prev, [key]: value }));
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    // Simulate API call
    await new Promise((resolve) => setTimeout(resolve, 800));

    const newVersion: ConfigVersion = {
      id: `v${configHistory.length + 1}`,
      author: "admin",
      timestamp: new Date().toISOString(),
      description: `Updated config: features=${features.filter((f) => f.enabled).length} enabled, maintenance=${maintenanceMode}`,
    };
    setConfigHistory((prev) => [newVersion, ...prev]);
    setLastSaved(new Date().toLocaleTimeString());
    setSaving(false);
  }, [features, maintenanceMode, configHistory.length]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-surface-900">System Configuration</h2>
          <p className="text-sm text-surface-800/50">Manage features, rate limits, and maintenance mode</p>
        </div>
        {lastSaved && (
          <span className="text-xs text-surface-800/40">Last saved: {lastSaved}</span>
        )}
      </div>

      {/* Maintenance mode */}
      <div className={`rounded-xl border p-5 transition-colors ${
        maintenanceMode
          ? "border-amber-300 bg-amber-50 dark:border-amber-700 dark:bg-amber-900/20"
          : "border-surface-200 bg-surface-0"
      }`}>
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-semibold text-surface-900 flex items-center gap-2">
              <svg className="w-4 h-4 text-amber-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                <line x1="12" y1="9" x2="12" y2="13" />
                <line x1="12" y1="17" x2="12.01" y2="17" />
              </svg>
              Maintenance Mode
            </h3>
            <p className="text-xs text-surface-800/50 mt-0.5">
              When enabled, all non-admin users see a maintenance page. API still accepts requests.
            </p>
          </div>
          <button
            onClick={() => setMaintenanceMode(!maintenanceMode)}
            className={`relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer ${
              maintenanceMode ? "bg-amber-500" : "bg-surface-300 dark:bg-surface-600"
            }`}
            role="switch"
            aria-checked={maintenanceMode}
          >
            <span
              className={`pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform ${
                maintenanceMode ? "translate-x-5" : "translate-x-0"
              }`}
            />
          </button>
        </div>
      </div>

      {/* Feature toggles */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <h3 className="text-sm font-semibold text-surface-900 mb-4">Feature Toggles</h3>
        <div className="space-y-3">
          {features.map((feature) => (
            <div
              key={feature.key}
              className="flex items-center justify-between gap-4 py-2"
            >
              <div className="min-w-0">
                <div className="text-sm font-medium text-surface-900">{feature.label}</div>
                <div className="text-xs text-surface-800/40">{feature.description}</div>
              </div>
              <button
                onClick={() => toggleFeature(feature.key)}
                className={`relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer ${
                  feature.enabled ? "bg-brand-600" : "bg-surface-300 dark:bg-surface-600"
                }`}
                role="switch"
                aria-checked={feature.enabled}
              >
                <span
                  className={`pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform ${
                    feature.enabled ? "translate-x-5" : "translate-x-0"
                  }`}
                />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Rate limits */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <h3 className="text-sm font-semibold text-surface-900 mb-4">Rate Limits</h3>
        <div className="space-y-4">
          <RateLimitRow
            label="Requests per Minute"
            description="Max API requests per minute per user"
            value={rateLimits.requestsPerMinute}
            onChange={(v) => updateRateLimit("requestsPerMinute", v)}
            min={1}
            max={1000}
          />
          <RateLimitRow
            label="Burst Allowance"
            description="Additional requests allowed in short bursts"
            value={rateLimits.burstAllowance}
            onChange={(v) => updateRateLimit("burstAllowance", v)}
            min={0}
            max={100}
          />
          <RateLimitRow
            label="Rental Requests per Hour"
            description="Max rental initiation requests per hour per user"
            value={rateLimits.rentalRequestsPerHour}
            onChange={(v) => updateRateLimit("rentalRequestsPerHour", v)}
            min={1}
            max={200}
          />
          <RateLimitRow
            label="API Calls per Day"
            description="Max total API calls per day per user"
            value={rateLimits.apiCallsPerDay}
            onChange={(v) => updateRateLimit("apiCallsPerDay", v)}
            min={100}
            max={1000000}
          />
        </div>
      </div>

      {/* Save button */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSave}
          disabled={saving}
          className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {saving ? "Saving..." : "Save Configuration"}
        </button>
      </div>

      {/* Config version history */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <h3 className="text-sm font-semibold text-surface-900 mb-4">Configuration History</h3>
        <div className="space-y-2">
          {configHistory.map((version) => (
            <div key={version.id} className="flex items-start gap-3 py-2 border-b border-surface-100 last:border-0 dark:border-surface-800">
              <span className="shrink-0 mt-0.5 inline-flex items-center justify-center h-6 w-6 rounded-full bg-surface-100 text-xs font-medium text-surface-800/60 dark:bg-surface-800 dark:text-surface-400">
                {version.id}
              </span>
              <div className="flex-1 min-w-0">
                <p className="text-sm text-surface-800/70">{version.description}</p>
                <p className="text-xs text-surface-800/40 mt-0.5">
                  by {version.author} · {new Date(version.timestamp).toLocaleString()}
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function RateLimitRow({
  label,
  description,
  value,
  onChange,
  min,
  max,
}: {
  label: string;
  description: string;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-sm font-medium text-surface-900">{label}</div>
        <div className="text-xs text-surface-800/40">{description}</div>
      </div>
      <div className="flex items-center gap-2">
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="w-32 h-1.5 rounded-full appearance-none bg-surface-200 accent-brand-600 cursor-pointer"
        />
        <input
          type="number"
          min={min}
          max={max}
          value={value}
          onChange={(e) => onChange(Math.max(min, Math.min(max, Number(e.target.value))))}
          className="w-24 px-2 py-1 text-sm rounded-md border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 text-right font-mono"
        />
      </div>
    </div>
  );
}
