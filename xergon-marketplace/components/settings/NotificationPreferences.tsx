"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type NotificationType =
  | "rental_started"
  | "rental_completed"
  | "rental_expiring"
  | "payment_received"
  | "new_model"
  | "price_change"
  | "provider_health"
  | "system";

export type { NotificationType };

type Channel = "in-app" | "email" | "push";
type DigestFrequency = "realtime" | "daily" | "weekly";

interface NotifTypeConfig {
  type: NotificationType;
  label: string;
  description: string;
  critical: boolean;
  channels: Record<Channel, boolean>;
}

interface QuietHours {
  enabled: boolean;
  startHour: number;
  endHour: number;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const NOTIFICATION_TYPES: Array<{ type: NotificationType; label: string; description: string; critical: boolean }> = [
  { type: "rental_started", label: "Rental Started", description: "When a new GPU rental begins", critical: false },
  { type: "rental_completed", label: "Rental Completed", description: "When a rental finishes", critical: false },
  { type: "rental_expiring", label: "Rental Expiring", description: "When a rental is about to expire", critical: true },
  { type: "payment_received", label: "Payment Received", description: "When you receive ERG payment", critical: true },
  { type: "new_model", label: "New Model Available", description: "When a new AI model is listed", critical: false },
  { type: "price_change", label: "Price Change", description: "When a model's pricing changes", critical: false },
  { type: "provider_health", label: "Provider Health Alert", description: "When your provider has health issues", critical: true },
  { type: "system", label: "System Notifications", description: "Platform announcements and updates", critical: false },
];

function getDefaultChannels(): Record<NotificationType, Record<Channel, boolean>> {
  const defaults: Record<NotificationType, Record<Channel, boolean>> = {} as any;
  for (const nt of NOTIFICATION_TYPES) {
    defaults[nt.type] = {
      "in-app": true,
      email: nt.critical,
      push: nt.critical,
    };
  }
  return defaults;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function NotificationPreferences() {
  const [channels, setChannels] = useState<Record<NotificationType, Record<Channel, boolean>>>(getDefaultChannels());
  const [digestFrequency, setDigestFrequency] = useState<DigestFrequency>("realtime");
  const [quietHours, setQuietHours] = useState<QuietHours>({
    enabled: false,
    startHour: 22,
    endHour: 7,
  });
  const [saving, setSaving] = useState(false);

  // Load from localStorage
  useEffect(() => {
    try {
      const saved = localStorage.getItem("xergon-notif-prefs");
      if (saved) {
        const data = JSON.parse(saved);
        if (data.channels) setChannels(data.channels);
        if (data.digestFrequency) setDigestFrequency(data.digestFrequency);
        if (data.quietHours) setQuietHours(data.quietHours);
      }
    } catch {
      // use defaults
    }
  }, []);

  const toggleChannel = useCallback((type: NotificationType, channel: Channel) => {
    setChannels((prev) => ({
      ...prev,
      [type]: {
        ...prev[type],
        [channel]: !prev[type][channel],
      },
    }));
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    await new Promise((r) => setTimeout(r, 600));

    const payload = { channels, digestFrequency, quietHours };
    localStorage.setItem("xergon-notif-prefs", JSON.stringify(payload));

    // In production, also POST to /api/user/notification-preferences
    try {
      await fetch("/api/user/notification-preferences", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      }).catch(() => { /* API may not exist yet */ });
    } catch {
      // ignore
    }

    setSaving(false);
    toast.success("Notification preferences saved");
  }, [channels, digestFrequency, quietHours]);

  return (
    <div className="space-y-6">
      {/* Digest frequency */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Digest Frequency</h2>
        <p className="text-sm text-surface-800/50 mb-4">
          How often you receive notification digests via email
        </p>
        <div className="flex flex-wrap items-center gap-2">
          {([
            { value: "realtime" as DigestFrequency, label: "Real-time", desc: "Instant notifications" },
            { value: "daily" as DigestFrequency, label: "Daily Digest", desc: "Once per day" },
            { value: "weekly" as DigestFrequency, label: "Weekly Digest", desc: "Once per week" },
          ]).map((opt) => (
            <button
              key={opt.value}
              onClick={() => setDigestFrequency(opt.value)}
              className={`px-4 py-2.5 rounded-lg border text-sm font-medium transition-colors ${
                digestFrequency === opt.value
                  ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-600"
                  : "border-surface-200 text-surface-800/60 hover:bg-surface-50 dark:border-surface-700 dark:hover:bg-surface-800"
              }`}
            >
              <div>{opt.label}</div>
              <div className="text-xs font-normal opacity-70">{opt.desc}</div>
            </button>
          ))}
        </div>
      </section>

      {/* Per-type channel toggles */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <h2 className="font-semibold mb-1">Notification Channels</h2>
        <p className="text-sm text-surface-800/50 mb-4">
          Choose how you receive each type of notification
        </p>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-200">
                <th className="text-left py-2 pr-4 font-medium text-surface-800/60">Type</th>
                <th className="text-center py-2 px-3 font-medium text-surface-800/60 w-24">In-App</th>
                <th className="text-center py-2 px-3 font-medium text-surface-800/60 w-24">Email</th>
                <th className="text-center py-2 px-3 font-medium text-surface-800/60 w-24">Push</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-surface-100 dark:divide-surface-800">
              {NOTIFICATION_TYPES.map((nt) => (
                <tr key={nt.type} className="hover:bg-surface-50/50 dark:hover:bg-surface-800/30">
                  <td className="py-3 pr-4">
                    <div className="flex items-center gap-2">
                      <p className="font-medium text-surface-900">{nt.label}</p>
                      {nt.critical && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300">
                          Critical
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-surface-800/40 mt-0.5">{nt.description}</p>
                  </td>
                  {(["in-app", "email", "push"] as Channel[]).map((channel) => (
                    <td key={channel} className="py-3 px-3 text-center">
                      <button
                        onClick={() => toggleChannel(nt.type, channel)}
                        className={`relative inline-flex h-5 w-9 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer mx-auto ${
                          channels[nt.type][channel] ? "bg-brand-600" : "bg-surface-300 dark:bg-surface-600"
                        }`}
                        role="switch"
                        aria-checked={channels[nt.type][channel]}
                        aria-label={`${nt.label} ${channel}`}
                      >
                        <span className={`pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow ring-0 transition-transform ${
                          channels[nt.type][channel] ? "translate-x-4" : "translate-x-0"
                        }`} />
                      </button>
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {/* Quiet hours */}
      <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="font-semibold">Quiet Hours</h2>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Disable non-critical notifications during set hours
            </p>
          </div>
          <button
            onClick={() => setQuietHours((prev) => ({ ...prev, enabled: !prev.enabled }))}
            className={`relative inline-flex h-6 w-11 shrink-0 rounded-full border-2 border-transparent transition-colors cursor-pointer ${
              quietHours.enabled ? "bg-brand-600" : "bg-surface-300 dark:bg-surface-600"
            }`}
            role="switch"
            aria-checked={quietHours.enabled}
          >
            <span className={`pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform ${
              quietHours.enabled ? "translate-x-5" : "translate-x-0"
            }`} />
          </button>
        </div>

        {quietHours.enabled && (
          <div className="flex items-center gap-3">
            <div>
              <label className="block text-xs text-surface-800/50 mb-1">From</label>
              <select
                value={quietHours.startHour}
                onChange={(e) => setQuietHours((prev) => ({ ...prev, startHour: Number(e.target.value) }))}
                className="px-2 py-1.5 text-sm rounded-md border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                {Array.from({ length: 24 }, (_, i) => (
                  <option key={i} value={i}>{String(i).padStart(2, "0")}:00</option>
                ))}
              </select>
            </div>
            <span className="text-surface-800/40 mt-4">to</span>
            <div>
              <label className="block text-xs text-surface-800/50 mb-1">To</label>
              <select
                value={quietHours.endHour}
                onChange={(e) => setQuietHours((prev) => ({ ...prev, endHour: Number(e.target.value) }))}
                className="px-2 py-1.5 text-sm rounded-md border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              >
                {Array.from({ length: 24 }, (_, i) => (
                  <option key={i} value={i}>{String(i).padStart(2, "0")}:00</option>
                ))}
              </select>
            </div>
            <p className="text-xs text-surface-800/40 mt-4">
              Critical notifications are never silenced
            </p>
          </div>
        )}
      </section>

      {/* Save */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSave}
          disabled={saving}
          className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 transition-colors disabled:opacity-50"
        >
          {saving ? "Saving..." : "Save Notification Preferences"}
        </button>
      </div>
    </div>
  );
}
