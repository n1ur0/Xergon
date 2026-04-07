"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import type { NotificationType } from "./NotificationPreferences";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type DigestFrequency = "daily" | "weekly" | "monthly";

interface EmailDigestConfig {
  enabled: boolean;
  frequency: DigestFrequency;
  includedTypes: NotificationType[];
  lastSentAt: string | null;
}

const ALL_NOTIF_TYPES: Array<{ type: NotificationType; label: string }> = [
  { type: "rental_started", label: "Rental Started" },
  { type: "rental_completed", label: "Rental Completed" },
  { type: "rental_expiring", label: "Rental Expiring" },
  { type: "payment_received", label: "Payment Received" },
  { type: "new_model", label: "New Model" },
  { type: "price_change", label: "Price Changes" },
  { type: "provider_health", label: "Provider Health" },
  { type: "system", label: "System Updates" },
];

// ---------------------------------------------------------------------------
// Sample digest data for preview
// ---------------------------------------------------------------------------

const SAMPLE_DIGEST = {
  date: new Date().toLocaleDateString("en-US", { weekday: "long", year: "numeric", month: "long", day: "numeric" }),
  newModels: [
    { name: "llama-3.1-405b", provider: "ProviderX", price: "0.008 ERG/1K tokens" },
    { name: "qwen-2.5-coder-32b", provider: "AiNode", price: "0.005 ERG/1K tokens" },
  ],
  priceChanges: [
    { model: "mistral-7b", oldPrice: "0.003 ERG/1K", newPrice: "0.002 ERG/1K", direction: "down" },
    { model: "deepseek-v3", oldPrice: "0.006 ERG/1K", newPrice: "0.007 ERG/1K", direction: "up" },
  ],
  providerUpdates: [
    { provider: "ProviderX", status: "Healthy", uptime: "99.9%" },
    { provider: "AiNode", status: "Degraded", uptime: "97.2%" },
  ],
  communityActivity: [
    { type: "review", text: "Alice left a 5-star review for ProviderX" },
    { type: "forum", text: "New discussion: 'Best practices for fine-tuning on Xergon'" },
  ],
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function EmailDigestSettings() {
  const [config, setConfig] = useState<EmailDigestConfig>({
    enabled: true,
    frequency: "daily",
    includedTypes: ALL_NOTIF_TYPES.map((t) => t.type),
    lastSentAt: "2026-04-04T08:00:00Z",
  });
  const [showPreview, setShowPreview] = useState(false);
  const [saving, setSaving] = useState(false);

  // Load from localStorage
  useEffect(() => {
    try {
      const saved = localStorage.getItem("xergon-email-digest");
      if (saved) {
        const data = JSON.parse(saved);
        setConfig((prev) => ({
          ...prev,
          ...data,
        }));
      }
    } catch {
      // use defaults
    }
  }, []);

  const toggleType = useCallback((type: NotificationType) => {
    setConfig((prev) => ({
      ...prev,
      includedTypes: prev.includedTypes.includes(type)
        ? prev.includedTypes.filter((t) => t !== type)
        : [...prev.includedTypes, type],
    }));
  }, []);

  const toggleAll = useCallback(() => {
    setConfig((prev) => ({
      ...prev,
      includedTypes: prev.includedTypes.length === ALL_NOTIF_TYPES.length
        ? []
        : ALL_NOTIF_TYPES.map((t) => t.type),
    }));
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    await new Promise((r) => setTimeout(r, 600));
    localStorage.setItem("xergon-email-digest", JSON.stringify(config));
    setSaving(false);
    toast.success("Email digest settings saved");
  }, [config]);

  const allSelected = config.includedTypes.length === ALL_NOTIF_TYPES.length;

  return (
    <div className="space-y-6">
      {/* Enable/disable */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="font-semibold">Email Digest</h2>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Receive a periodic email summary of your notifications
            </p>
          </div>
          <button
            onClick={() => setConfig((prev) => ({ ...prev, enabled: !prev.enabled }))}
            className={`relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer ${
              config.enabled ? "bg-brand-600" : "bg-surface-300 dark:bg-surface-600"
            }`}
            role="switch"
            aria-checked={config.enabled}
          >
            <span className={`pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform ${
              config.enabled ? "translate-x-5" : "translate-x-0"
            }`} />
          </button>
        </div>

        {config.enabled && (
          <>
            {/* Frequency */}
            <div className="mb-4">
              <label className="block text-sm font-medium text-surface-900 mb-2">Frequency</label>
              <div className="flex items-center gap-2">
                {([
                  { value: "daily" as DigestFrequency, label: "Daily" },
                  { value: "weekly" as DigestFrequency, label: "Weekly" },
                  { value: "monthly" as DigestFrequency, label: "Monthly" },
                ]).map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setConfig((prev) => ({ ...prev, frequency: opt.value }))}
                    className={`px-4 py-2 rounded-lg border text-sm font-medium transition-colors ${
                      config.frequency === opt.value
                        ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                        : "border-surface-200 text-surface-800/60 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
                    }`}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>

            {/* Last sent */}
            {config.lastSentAt && (
              <p className="text-xs text-surface-800/40">
                Last digest sent: {new Date(config.lastSentAt).toLocaleString()}
              </p>
            )}
          </>
        )}
      </section>

      {/* Content selection */}
      {config.enabled && (
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="font-semibold">Included Content</h2>
              <p className="text-sm text-surface-800/50 mt-0.5">
                Choose which notification types to include in the digest
              </p>
            </div>
            <button
              onClick={toggleAll}
              className="text-xs text-brand-600 hover:underline"
            >
              {allSelected ? "Deselect all" : "Select all"}
            </button>
          </div>
          <div className="flex flex-wrap gap-2">
            {ALL_NOTIF_TYPES.map((nt) => {
              const selected = config.includedTypes.includes(nt.type);
              return (
                <button
                  key={nt.type}
                  onClick={() => toggleType(nt.type)}
                  className={`px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors ${
                    selected
                      ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                      : "border-surface-200 text-surface-800/40 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
                  }`}
                >
                  {nt.label}
                </button>
              );
            })}
          </div>
        </section>
      )}

      {/* Preview */}
      {config.enabled && (
        <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="font-semibold">Digest Preview</h2>
              <p className="text-sm text-surface-800/50 mt-0.5">
                See what your email digest will look like
              </p>
            </div>
            <button
              onClick={() => setShowPreview(!showPreview)}
              className="text-xs text-brand-600 hover:underline"
            >
              {showPreview ? "Hide preview" : "Show preview"}
            </button>
          </div>

          {showPreview && (
            <div className="rounded-lg border border-surface-200 bg-white overflow-hidden dark:bg-surface-900 dark:border-surface-700">
              {/* Email header */}
              <div className="bg-brand-600 px-6 py-4 text-white">
                <h3 className="font-bold text-lg">Xergon Network</h3>
                <p className="text-sm text-brand-100">Your {config.frequency} digest — {SAMPLE_DIGEST.date}</p>
              </div>

              <div className="px-6 py-4 space-y-5">
                {/* New models */}
                <div>
                  <h4 className="text-sm font-semibold text-surface-900 mb-2">🆕 New Models</h4>
                  <div className="space-y-1.5">
                    {SAMPLE_DIGEST.newModels.map((m, i) => (
                      <div key={i} className="flex items-center justify-between text-sm py-1 border-b border-surface-100 last:border-0 dark:border-surface-800">
                        <span className="font-medium text-surface-800">{m.name}</span>
                        <span className="text-xs text-surface-800/50">{m.provider} · {m.price}</span>
                      </div>
                    ))}
                  </div>
                </div>

                {/* Price changes */}
                <div>
                  <h4 className="text-sm font-semibold text-surface-900 mb-2">💰 Price Changes</h4>
                  <div className="space-y-1.5">
                    {SAMPLE_DIGEST.priceChanges.map((p, i) => (
                      <div key={i} className="flex items-center justify-between text-sm py-1 border-b border-surface-100 last:border-0 dark:border-surface-800">
                        <span className="font-medium text-surface-800">{p.model}</span>
                        <span className={`text-xs font-medium ${p.direction === "down" ? "text-green-600" : "text-red-600"}`}>
                          {p.oldPrice} → {p.newPrice}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>

                {/* Provider updates */}
                <div>
                  <h4 className="text-sm font-semibold text-surface-900 mb-2">🖥️ Provider Updates</h4>
                  <div className="space-y-1.5">
                    {SAMPLE_DIGEST.providerUpdates.map((p, i) => (
                      <div key={i} className="flex items-center justify-between text-sm py-1 border-b border-surface-100 last:border-0 dark:border-surface-800">
                        <span className="font-medium text-surface-800">{p.provider}</span>
                        <span className={`text-xs font-medium ${p.status === "Healthy" ? "text-green-600" : "text-amber-600"}`}>
                          {p.status} · {p.uptime} uptime
                        </span>
                      </div>
                    ))}
                  </div>
                </div>

                {/* Community activity */}
                <div>
                  <h4 className="text-sm font-semibold text-surface-900 mb-2">💬 Community Activity</h4>
                  <div className="space-y-1.5">
                    {SAMPLE_DIGEST.communityActivity.map((a, i) => (
                      <div key={i} className="text-sm text-surface-800/70 py-1 border-b border-surface-100 last:border-0 dark:border-surface-800">
                        {a.text}
                      </div>
                    ))}
                  </div>
                </div>
              </div>

              {/* Email footer */}
              <div className="px-6 py-3 border-t border-surface-200 bg-surface-50 text-xs text-surface-800/40 dark:bg-surface-800/50 dark:border-surface-700">
                <div className="flex items-center justify-between">
                  <span>Xergon Network — Decentralized AI Marketplace</span>
                  <div className="flex items-center gap-3">
                    <a href="#" className="text-brand-600 hover:underline">Manage Preferences</a>
                    <a href="#" className="text-brand-600 hover:underline">Unsubscribe</a>
                  </div>
                </div>
              </div>
            </div>
          )}
        </section>
      )}

      {/* Save */}
      {config.enabled && (
        <div className="flex items-center gap-3">
          <button
            onClick={handleSave}
            disabled={saving}
            className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save Digest Settings"}
          </button>
        </div>
      )}
    </div>
  );
}
