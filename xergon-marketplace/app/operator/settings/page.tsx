"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { toast } from "sonner";
import { fetchProviders, type ProviderInfo } from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelPricing {
  id: string;
  modelName: string;
  pricePer1MTokens: number; // nanoERG
  currency: string;
  enabled: boolean;
}

interface ProviderSettings {
  name: string;
  endpoint: string;
  publicKey: string;
  status: string;
  region: string;
  alertNotifications: boolean;
  alertEmail: string;
  modelPricing: ModelPricing[];
}

const DEFAULT_SETTINGS: ProviderSettings = {
  name: "",
  endpoint: "",
  publicKey: "",
  status: "offline",
  region: "us-east",
  alertNotifications: true,
  alertEmail: "",
  modelPricing: [],
};

const REGIONS = [
  { value: "us-east", label: "US East" },
  { value: "us-west", label: "US West" },
  { value: "eu-west", label: "EU West" },
  { value: "eu-central", label: "EU Central" },
  { value: "asia-east", label: "Asia East" },
  { value: "asia-south", label: "Asia South" },
];

const STORAGE_KEY = "xergon-operator-settings";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function loadSettings(): ProviderSettings {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return DEFAULT_SETTINGS;
}

function saveSettings(settings: ProviderSettings) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

function nanoErgToErg(nano: number): string {
  return (nano / 1_000_000_000).toFixed(4);
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function OperatorSettingsPage() {
  const [settings, setSettings] = useState<ProviderSettings>(DEFAULT_SETTINGS);
  const [mounted, setMounted] = useState(false);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [ownProvider, setOwnProvider] = useState<ProviderInfo | null>(null);
  const [saving, setSaving] = useState(false);

  // Load from localStorage on mount
  useEffect(() => {
    setSettings(loadSettings());
    setMounted(true);
  }, []);

  // Fetch providers to find own provider
  const loadProviders = useCallback(async () => {
    try {
      const res = await fetchProviders();
      setProviders(res.providers);
      // Try to find own provider (match by endpoint or name from saved settings)
      if (settings.endpoint) {
        const found = res.providers.find(
          (p) => p.endpoint === settings.endpoint || p.name === settings.name
        );
        if (found) {
          setOwnProvider(found);
          setSettings((prev) => ({
            ...prev,
            name: found.name,
            endpoint: found.endpoint,
            status: found.status,
            publicKey: found.ergoAddress ?? prev.publicKey,
          }));
        }
      }
    } catch { /* ignore - just show what we have */ }
  }, [settings.endpoint, settings.name]);

  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------

  const updateField = <K extends keyof ProviderSettings>(key: K, value: ProviderSettings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
  };

  const addModelPricing = () => {
    const id = `model-${Date.now()}`;
    setSettings((prev) => ({
      ...prev,
      modelPricing: [
        ...prev.modelPricing,
        { id, modelName: "", pricePer1MTokens: 0, currency: "nanoERG", enabled: true },
      ],
    }));
  };

  const updateModelPricing = (id: string, field: keyof ModelPricing, value: string | number | boolean) => {
    setSettings((prev) => ({
      ...prev,
      modelPricing: prev.modelPricing.map((m) =>
        m.id === id ? { ...m, [field]: value } : m
      ),
    }));
  };

  const removeModelPricing = (id: string) => {
    setSettings((prev) => ({
      ...prev,
      modelPricing: prev.modelPricing.filter((m) => m.id !== id),
    }));
  };

  const handleSave = async () => {
    setSaving(true);
    await new Promise((r) => setTimeout(r, 600));
    saveSettings(settings);
    setSaving(false);
    toast.success("Settings saved to local storage");
  };

  const handleReset = () => {
    setSettings(DEFAULT_SETTINGS);
    localStorage.removeItem(STORAGE_KEY);
    toast.info("Settings reset to defaults");
  };

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  if (!mounted) {
    return (
      <div className="max-w-3xl space-y-6 animate-pulse">
        <div className="h-8 w-48 bg-surface-200 rounded" />
        <div className="h-64 bg-surface-200 rounded-xl" />
        <div className="h-48 bg-surface-200 rounded-xl" />
      </div>
    );
  }

  return (
    <div className="max-w-3xl space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-surface-900">Operator Settings</h1>
        <p className="text-sm text-surface-800/50 mt-1">
          Configure your provider, pricing, models, and notifications.
        </p>
      </div>

      {/* Provider Info */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
        <div>
          <h2 className="text-base font-semibold text-surface-900">Provider Info</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">Your registered provider details from the network.</p>
        </div>

        {ownProvider ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
            <div>
              <span className="block text-surface-800/50 mb-1">Name</span>
              <p className="font-medium text-surface-900">{ownProvider.name}</p>
            </div>
            <div>
              <span className="block text-surface-800/50 mb-1">Status</span>
              <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${
                ownProvider.status === "online"
                  ? "bg-accent-100 text-accent-700"
                  : ownProvider.status === "degraded"
                    ? "bg-yellow-100 text-yellow-700"
                    : "bg-surface-200 text-surface-800/40"
              }`}>
                <span className={`h-1.5 w-1.5 rounded-full ${
                  ownProvider.status === "online" ? "bg-accent-500" : ownProvider.status === "degraded" ? "bg-yellow-500" : "bg-surface-400"
                }`} />
                {ownProvider.status}
              </span>
            </div>
            <div className="sm:col-span-2">
              <span className="block text-surface-800/50 mb-1">Endpoint</span>
              <p className="font-mono text-xs bg-surface-100 rounded-lg px-3 py-2 break-all">{ownProvider.endpoint}</p>
            </div>
            {ownProvider.ergoAddress && (
              <div className="sm:col-span-2">
                <span className="block text-surface-800/50 mb-1">Public Key (Ergo Address)</span>
                <p className="font-mono text-xs bg-surface-100 rounded-lg px-3 py-2 break-all">{ownProvider.ergoAddress}</p>
              </div>
            )}
          </div>
        ) : (
          <div className="text-sm text-surface-800/50 bg-surface-50 rounded-lg p-4">
            <p>No provider matched your saved endpoint. Configure your provider endpoint below or register on the network.</p>
            {providers.length > 0 && (
              <div className="mt-3">
                <label className="block text-xs text-surface-800/40 mb-1">Select your provider from the network</label>
                <select
                  value={settings.endpoint}
                  onChange={(e) => {
                    const selected = providers.find((p) => p.endpoint === e.target.value);
                    if (selected) {
                      setSettings((prev) => ({
                        ...prev,
                        name: selected.name,
                        endpoint: selected.endpoint,
                        status: selected.status,
                        publicKey: selected.ergoAddress ?? "",
                      }));
                      setOwnProvider(selected);
                    }
                  }}
                  className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
                >
                  <option value="">-- Select a provider --</option>
                  {providers.map((p) => (
                    <option key={p.endpoint} value={p.endpoint}>
                      {p.name} ({p.status})
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>
        )}

        <SettingsRow
          label="Provider Endpoint"
          description="Your inference server endpoint URL"
          input={
            <input
              type="text"
              placeholder="https://your-node.xergon.network/v1"
              value={settings.endpoint}
              onChange={(e) => updateField("endpoint", e.target.value)}
              className="field-input w-64"
            />
          }
        />

        <SettingsRow
          label="Public Key"
          description="Your Ergo wallet address for payments"
          input={
            <input
              type="text"
              placeholder="9f...your-address"
              value={settings.publicKey}
              onChange={(e) => updateField("publicKey", e.target.value)}
              className="field-input w-64"
            />
          }
        />
      </section>

      {/* Pricing Management */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-base font-semibold text-surface-900">Pricing Management</h2>
            <p className="text-xs text-surface-800/40 mt-0.5">Set per-model pricing in nanoERG per 1M tokens.</p>
          </div>
          <button
            type="button"
            onClick={addModelPricing}
            className="inline-flex items-center gap-1.5 rounded-lg border border-surface-200 px-3 py-1.5 text-xs font-medium text-surface-800/70 hover:bg-surface-100 transition-colors"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            Add Model
          </button>
        </div>

        {settings.modelPricing.length === 0 ? (
          <div className="text-sm text-surface-800/40 text-center py-6 bg-surface-50 rounded-lg">
            No pricing rules configured. Click "Add Model" to get started.
          </div>
        ) : (
          <div className="space-y-3">
            {settings.modelPricing.map((model) => (
              <div
                key={model.id}
                className="flex flex-col sm:flex-row sm:items-center gap-3 p-3 rounded-lg bg-surface-50 border border-surface-100"
              >
                <div className="flex-1 min-w-0">
                  <input
                    type="text"
                    placeholder="Model name (e.g. llama-3.1-70b)"
                    value={model.modelName}
                    onChange={(e) => updateModelPricing(model.id, "modelName", e.target.value)}
                    className="field-input w-full text-sm"
                  />
                </div>
                <div className="flex items-center gap-2">
                  <div className="relative w-36">
                    <input
                      type="number"
                      min={0}
                      placeholder="0"
                      value={model.pricePer1MTokens || ""}
                      onChange={(e) => updateModelPricing(model.id, "pricePer1MTokens", parseInt(e.target.value) || 0)}
                      className="field-input w-full pr-16 text-sm"
                    />
                    <span className="absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-surface-800/40">nanoERG</span>
                  </div>
                  {model.pricePer1MTokens > 0 && (
                    <span className="text-[10px] text-surface-800/30 whitespace-nowrap">
                      ({nanoErgToErg(model.pricePer1MTokens)} ERG)
                    </span>
                  )}
                  <select
                    value={model.currency}
                    onChange={(e) => updateModelPricing(model.id, "currency", e.target.value)}
                    className="rounded-lg border border-surface-200 bg-surface-0 px-2 py-2 text-xs text-surface-800/70 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
                  >
                    <option value="nanoERG">nanoERG</option>
                    <option value="ERG">ERG</option>
                  </select>
                  <button
                    type="button"
                    onClick={() => updateModelPricing(model.id, "enabled", !model.enabled)}
                    className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-brand-500/30 ${
                      model.enabled ? "bg-brand-600" : "bg-surface-300"
                    }`}
                    role="switch"
                    aria-checked={model.enabled}
                  >
                    <span className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                      model.enabled ? "translate-x-5" : "translate-x-0"
                    }`} />
                  </button>
                  <button
                    type="button"
                    onClick={() => removeModelPricing(model.id)}
                    className="text-surface-400 hover:text-danger-500 transition-colors p-1"
                    title="Remove"
                  >
                    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Model Configuration */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
        <div>
          <h2 className="text-base font-semibold text-surface-900">Model Configuration</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">Enable or disable models served by your provider.</p>
        </div>

        {ownProvider && ownProvider.models.length > 0 ? (
          <div className="space-y-2">
            {ownProvider.models.map((modelName) => {
              const pricing = settings.modelPricing.find((m) => m.modelName === modelName);
              const enabled = pricing ? pricing.enabled : true;
              return (
                <div key={modelName} className="flex items-center justify-between gap-4 py-2 px-3 rounded-lg hover:bg-surface-50 transition-colors">
                  <div className="min-w-0">
                    <div className="text-sm font-mono font-medium text-surface-900">{modelName}</div>
                    <div className="text-xs text-surface-800/40">
                      {pricing && pricing.pricePer1MTokens > 0
                        ? `${nanoErgToErg(pricing.pricePer1MTokens)} ERG / 1M tokens`
                        : "No pricing set"}
                    </div>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={enabled}
                    onClick={() => {
                      if (pricing) {
                        updateModelPricing(pricing.id, "enabled", !enabled);
                      } else {
                        // Auto-add to pricing list
                        const id = `model-${Date.now()}`;
                        setSettings((prev) => ({
                          ...prev,
                          modelPricing: [
                            ...prev.modelPricing,
                            { id, modelName, pricePer1MTokens: 0, currency: "nanoERG", enabled: false },
                          ],
                        }));
                      }
                    }}
                    className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-brand-500/30 ${
                      enabled ? "bg-brand-600" : "bg-surface-300"
                    }`}
                  >
                    <span className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                      enabled ? "translate-x-5" : "translate-x-0"
                    }`} />
                  </button>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-sm text-surface-800/40 text-center py-6 bg-surface-50 rounded-lg">
            {ownProvider
              ? "Your provider is not currently serving any models."
              : "Link your provider above to see and manage served models."}
          </div>
        )}
      </section>

      {/* Region Configuration */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
        <div>
          <h2 className="text-base font-semibold text-surface-900">Region Configuration</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">Set your provider region for routing optimization.</p>
        </div>

        <SettingsRow
          label="Provider Region"
          description="The geographic region where your inference servers are located"
          input={
            <select
              value={settings.region}
              onChange={(e) => updateField("region", e.target.value)}
              className="field-input w-48"
            >
              {REGIONS.map((r) => (
                <option key={r.value} value={r.value}>{r.label}</option>
              ))}
            </select>
          }
        />
      </section>

      {/* Notification Settings */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
        <div>
          <h2 className="text-base font-semibold text-surface-900">Notifications</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">Configure alert preferences (email alerts coming soon).</p>
        </div>

        <NotificationRow
          label="Alert Notifications"
          description="Receive alerts for provider status changes and errors"
          checked={settings.alertNotifications}
          onChange={() => updateField("alertNotifications", !settings.alertNotifications)}
        />

        <SettingsRow
          label="Alert Email Address"
          description="Email address for alert notifications (future)"
          input={
            <input
              type="email"
              placeholder="operator@example.com"
              value={settings.alertEmail}
              onChange={(e) => updateField("alertEmail", e.target.value)}
              className="field-input w-64"
            />
          }
        />
      </section>

      {/* Actions */}
      <div className="flex items-center justify-between pt-2">
        <button
          type="button"
          onClick={handleReset}
          className="inline-flex items-center gap-2 rounded-lg border border-surface-200 px-5 py-2.5 text-sm font-medium text-surface-800/70 hover:bg-surface-100 transition-colors"
        >
          Reset to Defaults
        </button>
        <button
          type="button"
          onClick={handleSave}
          disabled={saving}
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-6 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
        >
          {saving && (
            <svg className="animate-spin w-4 h-4" viewBox="0 0 24 24" fill="none">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          )}
          {saving ? "Saving..." : "Save Settings"}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function SettingsRow({ label, description, input }: { label: string; description: string; input: React.ReactNode }) {
  return (
    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 sm:gap-6 py-2">
      <div className="min-w-0">
        <div className="text-sm font-medium text-surface-900">{label}</div>
        <div className="text-xs text-surface-800/40">{description}</div>
      </div>
      <div className="flex-shrink-0">{input}</div>
    </div>
  );
}

function NotificationRow({ label, description, checked, onChange }: { label: string; description: string; checked: boolean; onChange: () => void }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2">
      <div className="min-w-0">
        <div className="text-sm font-medium text-surface-900">{label}</div>
        <div className="text-xs text-surface-800/40">{description}</div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={onChange}
        className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-brand-500/30 ${
          checked ? "bg-brand-600" : "bg-surface-300"
        }`}
      >
        <span className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
          checked ? "translate-x-5" : "translate-x-0"
        }`} />
      </button>
    </div>
  );
}
