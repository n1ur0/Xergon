"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { toast } from "sonner";
import type { ProviderInfo } from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface HealthCheck {
  score: number;
  latency: number;
  errorRate: number;
  timestamp: string;
}

interface AlertRule {
  id: string;
  providerEndpoint: string;
  providerName: string;
  metric: "health_score" | "latency" | "error_rate" | "downtime";
  condition: "below" | "above";
  threshold: number;
  enabled: boolean;
}

interface AlertEvent {
  id: string;
  ruleId: string;
  providerName: string;
  providerEndpoint: string;
  metric: string;
  value: number;
  threshold: number;
  condition: string;
  timestamp: string;
  acknowledged: boolean;
}

// ---------------------------------------------------------------------------
// LocalStorage helpers
// ---------------------------------------------------------------------------

const ALERT_RULES_KEY = "xergon_alert_rules";
const ALERT_HISTORY_KEY = "xergon_alert_history";

function loadAlertRules(): AlertRule[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(ALERT_RULES_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveAlertRules(rules: AlertRule[]) {
  if (typeof window === "undefined") return;
  localStorage.setItem(ALERT_RULES_KEY, JSON.stringify(rules));
}

function loadAlertHistory(): AlertEvent[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(ALERT_HISTORY_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveAlertHistory(events: AlertEvent[]) {
  if (typeof window === "undefined") return;
  // Keep last 200 alerts
  const trimmed = events.slice(0, 200);
  localStorage.setItem(ALERT_HISTORY_KEY, JSON.stringify(trimmed));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

const STATUS_COLORS: Record<string, string> = {
  online: "bg-accent-100 text-accent-700",
  degraded: "bg-yellow-100 text-yellow-700",
  offline: "bg-surface-200 text-surface-800/40",
};

const STATUS_DOT: Record<string, string> = {
  online: "bg-accent-500",
  degraded: "bg-yellow-500",
  offline: "bg-surface-400",
};

const METRIC_LABELS: Record<string, string> = {
  health_score: "Health Score",
  latency: "Latency (ms)",
  error_rate: "Error Rate (%)",
  downtime: "Downtime (min)",
};

const CONDITION_LABELS: Record<string, string> = {
  below: "drops below",
  above: "exceeds",
};

// ---------------------------------------------------------------------------
// Sparkline component (mini chart)
// ---------------------------------------------------------------------------

function Sparkline({ data, color, height = 32 }: { data: number[]; color: string; height?: number }) {
  if (data.length < 2) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const w = 100;
  const h = height;
  const points = data.map((v, i) => {
    const x = (i / (data.length - 1)) * w;
    const y = h - ((v - min) / range) * h;
    return `${x},${y}`;
  }).join(" ");

  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="w-full" style={{ height }} preserveAspectRatio="none">
      <polyline points={points} fill="none" stroke={color} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function AlertsPage() {
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Health check history per provider (simulated from uptime history)
  const [healthHistory, setHealthHistory] = useState<Record<string, HealthCheck[]>>({});

  const [alertRules, setAlertRules] = useState<AlertRule[]>([]);
  const [alertHistory, setAlertHistory] = useState<AlertEvent[]>([]);

  // Alert config form
  const [showForm, setShowForm] = useState(false);
  const [formProvider, setFormProvider] = useState("");
  const [formMetric, setFormMetric] = useState<AlertRule["metric"]>("health_score");
  const [formCondition, setFormCondition] = useState<AlertRule["condition"]>("below");
  const [formThreshold, setFormThreshold] = useState(80);

  const checkIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Load providers
  const loadProviders = useCallback(async () => {
    try {
      const res = await fetch("/api/operator/providers");
      if (!res.ok) throw new Error(`API returned ${res.status}`);
      const data = await res.json();
      const list: ProviderInfo[] = Array.isArray(data) ? data : data.providers ?? [];
      setProviders(list);

      // Build simulated health history from uptime history or random data
      const history: Record<string, HealthCheck[]> = {};
      for (const p of list) {
        const prev = healthHistory[p.endpoint] ?? [];
        const checks = p.uptimeHistory
          ? p.uptimeHistory.slice(-20).map((u, i) => ({
              score: u,
              latency: p.latencyMs + Math.floor(Math.random() * 40 - 20),
              errorRate: +(Math.random() * 4).toFixed(2),
              timestamp: new Date(Date.now() - (20 - i) * 30_000).toISOString(),
            }))
          : prev.length > 0
            ? prev.slice(-19).concat([{
                score: p.status === "online" ? 90 + Math.floor(Math.random() * 10) : p.status === "degraded" ? 50 + Math.floor(Math.random() * 30) : Math.floor(Math.random() * 30),
                latency: p.latencyMs + Math.floor(Math.random() * 40 - 20),
                errorRate: +(Math.random() * 4).toFixed(2),
                timestamp: new Date().toISOString(),
              }])
            : Array.from({ length: 20 }, (_, i) => ({
                score: p.status === "online" ? 85 + Math.floor(Math.random() * 15) : p.status === "degraded" ? 50 + Math.floor(Math.random() * 30) : Math.floor(Math.random() * 30),
                latency: p.latencyMs + Math.floor(Math.random() * 40 - 20),
                errorRate: +(Math.random() * 4).toFixed(2),
                timestamp: new Date(Date.now() - (20 - i) * 30_000).toISOString(),
              }));
        history[p.endpoint] = checks;
      }
      setHealthHistory(history);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, [healthHistory]);

  useEffect(() => {
    setAlertRules(loadAlertRules());
    setAlertHistory(loadAlertHistory());
    loadProviders();

    const interval = setInterval(loadProviders, 30_000);
    checkIntervalRef.current = interval;
    return () => {
      if (checkIntervalRef.current) clearInterval(checkIntervalRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Evaluate alert rules against current data
  useEffect(() => {
    if (providers.length === 0 || alertRules.length === 0) return;

    const newAlerts: AlertEvent[] = [];

    for (const rule of alertRules) {
      if (!rule.enabled) continue;
      const provider = providers.find((p) => p.endpoint === rule.providerEndpoint);
      if (!provider) continue;

      const checks = healthHistory[rule.providerEndpoint];
      if (!checks || checks.length === 0) continue;
      const latest = checks[checks.length - 1];

      let value = 0;
      switch (rule.metric) {
        case "health_score":
          value = latest.score;
          break;
        case "latency":
          value = latest.latency;
          break;
        case "error_rate":
          value = latest.errorRate;
          break;
        case "downtime":
          value = 100 - latest.score; // Simplified proxy
          break;
      }

      const triggered =
        (rule.condition === "below" && value < rule.threshold) ||
        (rule.condition === "above" && value > rule.threshold);

      if (triggered) {
        // Avoid duplicate alerts for the same rule within 5 minutes
        const recent = alertHistory.find(
          (a) => a.ruleId === rule.id && Date.now() - new Date(a.timestamp).getTime() < 300_000
        );
        if (!recent) {
          newAlerts.push({
            id: `alert-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            ruleId: rule.id,
            providerName: rule.providerName,
            providerEndpoint: rule.providerEndpoint,
            metric: rule.metric,
            value,
            threshold: rule.threshold,
            condition: rule.condition,
            timestamp: new Date().toISOString(),
            acknowledged: false,
          });
        }
      }
    }

    if (newAlerts.length > 0) {
      const updated = [...newAlerts, ...alertHistory];
      setAlertHistory(updated);
      saveAlertHistory(updated);
      toast.warning(`${newAlerts.length} alert(s) triggered`, {
        description: newAlerts.map((a) => `${a.providerName}: ${METRIC_LABELS[a.metric]} ${CONDITION_LABELS[a.condition]} ${a.threshold}`).join(", "),
      });
    }
  }, [providers, healthHistory, alertRules, alertHistory]);

  // Add alert rule
  const handleAddRule = () => {
    const provider = providers.find((p) => p.endpoint === formProvider);
    if (!provider) return;

    const rule: AlertRule = {
      id: `rule-${Date.now()}`,
      providerEndpoint: formProvider,
      providerName: provider.name,
      metric: formMetric,
      condition: formCondition,
      threshold: formThreshold,
      enabled: true,
    };

    const updated = [...alertRules, rule];
    setAlertRules(updated);
    saveAlertRules(updated);
    setShowForm(false);
    toast.success("Alert rule created");
  };

  // Delete alert rule
  const handleDeleteRule = (id: string) => {
    const updated = alertRules.filter((r) => r.id !== id);
    setAlertRules(updated);
    saveAlertRules(updated);
    toast.success("Alert rule removed");
  };

  // Toggle rule
  const handleToggleRule = (id: string) => {
    const updated = alertRules.map((r) => r.id === id ? { ...r, enabled: !r.enabled } : r);
    setAlertRules(updated);
    saveAlertRules(updated);
  };

  // Acknowledge alert
  const handleAcknowledge = (id: string) => {
    const updated = alertHistory.map((a) => a.id === id ? { ...a, acknowledged: true } : a);
    setAlertHistory(updated);
    saveAlertHistory(updated);
  };

  // Clear all alerts
  const handleClearHistory = () => {
    setAlertHistory([]);
    saveAlertHistory([]);
    toast.success("Alert history cleared");
  };

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  const unacknowledgedCount = alertHistory.filter((a) => !a.acknowledged).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Provider Monitoring & Alerts</h1>
          <p className="text-sm text-surface-800/50 mt-1">
            Monitor provider health in real-time and configure alert thresholds.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {unacknowledgedCount > 0 && (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-yellow-100 px-3 py-1 text-xs font-medium text-yellow-700">
              <span className="h-1.5 w-1.5 rounded-full bg-yellow-500 animate-pulse" />
              {unacknowledgedCount} unacknowledged
            </span>
          )}
          <button
            onClick={() => setShowForm((p) => !p)}
            className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            New Alert Rule
          </button>
        </div>
      </div>

      {/* New Alert Rule Form */}
      {showForm && (
        <div className="rounded-xl border border-brand-200 bg-brand-50/30 p-5 space-y-4">
          <h3 className="text-sm font-semibold text-surface-900">Create Alert Rule</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4">
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Provider</label>
              <select
                value={formProvider}
                onChange={(e) => setFormProvider(e.target.value)}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
              >
                <option value="">Select provider...</option>
                {providers.map((p) => (
                  <option key={p.endpoint} value={p.endpoint}>{p.name}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Metric</label>
              <select
                value={formMetric}
                onChange={(e) => setFormMetric(e.target.value as AlertRule["metric"])}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
              >
                {Object.entries(METRIC_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>{label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Condition</label>
              <select
                value={formCondition}
                onChange={(e) => setFormCondition(e.target.value as AlertRule["condition"])}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
              >
                <option value="below">Drops below</option>
                <option value="above">Exceeds</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Threshold</label>
              <input
                type="number"
                value={formThreshold}
                onChange={(e) => setFormThreshold(parseFloat(e.target.value) || 0)}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
              />
            </div>
            <div className="flex items-end gap-2">
              <button
                onClick={handleAddRule}
                disabled={!formProvider}
                className="flex-1 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Create
              </button>
              <button
                onClick={() => setShowForm(false)}
                className="rounded-lg border border-surface-200 px-4 py-2 text-sm font-medium text-surface-800/60 hover:bg-surface-100 transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Provider Health Cards */}
      <div>
        <h2 className="text-base font-semibold text-surface-900 mb-3">Provider Health</h2>
        {error && !providers.length && (
          <div className="rounded-xl border border-danger-200 bg-danger-50/50 p-6 text-center">
            <p className="text-sm text-danger-700">Failed to load providers: {error}</p>
          </div>
        )}
        {loading && !providers.length && (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 animate-pulse">
            {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="h-48 bg-surface-200 rounded-xl" />
            ))}
          </div>
        )}
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {providers.map((p) => {
            const checks = healthHistory[p.endpoint] ?? [];
            const latest = checks[checks.length - 1];
            const scoreColor = (latest?.score ?? 0) >= 85 ? "#22c55e" : (latest?.score ?? 0) >= 60 ? "#eab308" : "#ef4444";
            const latencyData = checks.map((c) => c.latency);
            const scoreData = checks.map((c) => c.score);
            const avgErrorRate = checks.length > 0
              ? (checks.reduce((s, c) => s + c.errorRate, 0) / checks.length).toFixed(2)
              : "0.00";

            return (
              <div key={p.endpoint} className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3">
                {/* Header */}
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-surface-900 truncate">{p.name}</p>
                    <p className="text-xs text-surface-800/40 font-mono truncate">{p.endpoint}</p>
                  </div>
                  <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium flex-shrink-0 ${STATUS_COLORS[p.status] ?? STATUS_COLORS.offline}`}>
                    <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[p.status] ?? STATUS_DOT.offline}`} />
                    {p.status}
                  </span>
                </div>

                {/* Health Score Trend */}
                <div>
                  <div className="flex items-center justify-between text-xs mb-1">
                    <span className="text-surface-800/50">Health Score</span>
                    <span className="font-medium" style={{ color: scoreColor }}>{latest?.score ?? "--"}</span>
                  </div>
                  {scoreData.length > 1 && (
                    <Sparkline data={scoreData} color={scoreColor} height={28} />
                  )}
                </div>

                {/* Latency Trend */}
                <div>
                  <div className="flex items-center justify-between text-xs mb-1">
                    <span className="text-surface-800/50">Latency</span>
                    <span className="font-medium text-surface-800">{latest?.latency ?? "--"}ms</span>
                  </div>
                  {latencyData.length > 1 && (
                    <Sparkline data={latencyData} color="#6366f1" height={28} />
                  )}
                </div>

                {/* Stats row */}
                <div className="flex items-center justify-between text-xs text-surface-800/50 pt-1 border-t border-surface-100">
                  <span>Error Rate: <span className="font-medium text-surface-800">{avgErrorRate}%</span></span>
                  <span>Seen: <span className="font-medium text-surface-800">{timeAgo(p.lastSeen)}</span></span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Active Alert Rules */}
      <div>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-semibold text-surface-900">Alert Rules ({alertRules.length})</h2>
        </div>
        {alertRules.length === 0 ? (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
            <svg className="mx-auto w-8 h-8 text-surface-800/20 mb-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9" />
            </svg>
            <p className="text-sm text-surface-800/50">No alert rules configured.</p>
            <p className="text-xs text-surface-800/30 mt-1">Create a rule above to get notified when providers degrade.</p>
          </div>
        ) : (
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 text-left bg-surface-50">
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Provider</th>
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Metric</th>
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Condition</th>
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Threshold</th>
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Status</th>
                  <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50 text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {alertRules.map((rule) => (
                  <tr key={rule.id} className="border-b border-surface-100 hover:bg-surface-50 transition-colors">
                    <td className="px-4 py-3 font-medium">{rule.providerName}</td>
                    <td className="px-4 py-3 text-surface-800/60">{METRIC_LABELS[rule.metric]}</td>
                    <td className="px-4 py-3 text-surface-800/60">{CONDITION_LABELS[rule.condition]}</td>
                    <td className="px-4 py-3 font-mono">{rule.threshold}</td>
                    <td className="px-4 py-3">
                      <button
                        onClick={() => handleToggleRule(rule.id)}
                        role="switch"
                        aria-checked={rule.enabled}
                        className={`relative inline-flex h-5 w-9 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-brand-500/30 ${
                          rule.enabled ? "bg-brand-600" : "bg-surface-300"
                        }`}
                      >
                        <span className={`pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                          rule.enabled ? "translate-x-4" : "translate-x-0"
                        }`} />
                      </button>
                    </td>
                    <td className="px-4 py-3 text-right">
                      <button
                        onClick={() => handleDeleteRule(rule.id)}
                        className="text-xs text-danger-500 hover:text-danger-700 font-medium"
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Alert History */}
      <div>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-semibold text-surface-900">Alert History ({alertHistory.length})</h2>
          {alertHistory.length > 0 && (
            <button
              onClick={handleClearHistory}
              className="text-xs text-surface-800/50 hover:text-danger-500 font-medium transition-colors"
            >
              Clear All
            </button>
          )}
        </div>
        {alertHistory.length === 0 ? (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
            <p className="text-sm text-surface-800/50">No alerts triggered yet.</p>
            <p className="text-xs text-surface-800/30 mt-1">Alerts will appear here when rules are triggered.</p>
          </div>
        ) : (
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <div className="max-h-96 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="sticky top-0">
                  <tr className="border-b border-surface-200 text-left bg-surface-50">
                    <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Time</th>
                    <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Provider</th>
                    <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Alert</th>
                    <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50">Value</th>
                    <th className="px-4 py-2.5 text-xs font-medium text-surface-800/50 text-right">Action</th>
                  </tr>
                </thead>
                <tbody>
                  {alertHistory.map((evt) => (
                    <tr key={evt.id} className={`border-b border-surface-100 transition-colors ${!evt.acknowledged ? "bg-yellow-50/50" : "hover:bg-surface-50"}`}>
                      <td className="px-4 py-3 text-xs text-surface-800/50 whitespace-nowrap">{timeAgo(evt.timestamp)}</td>
                      <td className="px-4 py-3 font-medium">{evt.providerName}</td>
                      <td className="px-4 py-3 text-surface-800/60">
                        <span className="text-xs">{METRIC_LABELS[evt.metric]}</span>
                        <span className="text-xs text-surface-800/40 ml-1">{CONDITION_LABELS[evt.condition]} {evt.threshold}</span>
                      </td>
                      <td className="px-4 py-3 font-mono text-xs">{evt.value}</td>
                      <td className="px-4 py-3 text-right">
                        {evt.acknowledged ? (
                          <span className="text-xs text-surface-800/30">Acknowledged</span>
                        ) : (
                          <button
                            onClick={() => handleAcknowledge(evt.id)}
                            className="text-xs text-brand-600 hover:text-brand-700 font-medium"
                          >
                            Acknowledge
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
