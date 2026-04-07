"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Provider {
  id: string;
  name: string;
  region: string;
  status: "online" | "degraded" | "offline";
  healthScore: number;
  activeTasks: number;
  latencyP50: number;
  uptime: number;
}

interface EventItem {
  id: string;
  type: "provider_joined" | "provider_left" | "model_added" | "model_removed" | "circuit_breaker" | "governance";
  message: string;
  timestamp: string;
}

interface DashboardData {
  systemHealth: "green" | "yellow" | "red";
  activeProviders: number;
  requestsPerMin: number;
  latencyP50: number;
  latencyP95: number;
  latencyP99: number;
  errorRate: number;
  revenueToday: number;
  revenueWeek: number;
  revenueMonth: number;
  providers: Provider[];
  requestHistory: number[];
  modelDistribution: { model: string; percentage: number; requests: number }[];
  events: EventItem[];
}

// ---------------------------------------------------------------------------
// Simulated data
// ---------------------------------------------------------------------------

function generateData(): DashboardData {
  const providerNames = ["GPU Node Alpha", "Tensor Hub West", "Neural Cloud EU", "InferenceMax", "DeepServe APAC", "FlashCompute", "QuantumGPU", "MegaNode", "RapidAI", "CloudInfer"];
  const regions = ["US-East", "US-West", "EU-West", "EU-Central", "Asia-Pacific", "South America"];
  const statuses: Array<Provider["status"]> = ["online", "online", "online", "online", "online", "degraded", "online", "online", "offline", "online"];

  const providers: Provider[] = providerNames.map((name, i) => ({
    id: `prov-${i + 1}`,
    name,
    region: regions[i % regions.length],
    status: statuses[i],
    healthScore: Math.floor(Math.random() * 30 + 70),
    activeTasks: Math.floor(Math.random() * 50 + 5),
    latencyP50: Math.floor(Math.random() * 150 + 50),
    uptime: +(Math.random() * 5 + 95).toFixed(1),
  }));

  const requestHistory = Array.from({ length: 60 }, () => Math.floor(Math.random() * 200 + 100));

  const models = ["llama-3.1-70b", "llama-3.1-8b", "mixtral-8x7b", "mistral-7b", "qwen-2.5-72b", "codestral-22b", "deepseek-coder-33b", "phi-3-medium"];
  const totalRequests = models.reduce(() => Math.floor(Math.random() * 5000 + 1000), 0);
  let remaining = totalRequests;
  const modelDistribution = models.map((model, i) => {
    const isLast = i === models.length - 1;
    const requests = isLast ? remaining : Math.floor(Math.random() * (remaining / 2) + 200);
    remaining -= requests;
    return { model, percentage: 0, requests };
  });
  const totalModelReqs = modelDistribution.reduce((s, m) => s + m.requests, 0);
  modelDistribution.forEach((m) => { m.percentage = +((m.requests / totalModelReqs) * 100).toFixed(1); });
  modelDistribution.sort((a, b) => b.requests - a.requests);

  const eventTypes: Array<EventItem["type"]> = ["provider_joined", "model_added", "circuit_breaker", "governance", "provider_left", "model_removed"];
  const eventMessages: Record<EventItem["type"], string[]> = {
    provider_joined: ["GPU Node Alpha came online", "FlashCompute connected from US-West", "New provider registered in EU-Central"],
    provider_left: ["Neural Cloud EU went offline", "RapidAI disconnected", "QuantumGPU entered maintenance"],
    model_added: ["llama-3.1-405b added to InferenceMax", "flux-1 now available on DeepServe", "qwen-2.5-coder added"],
    model_removed: ["deprecated model removed from FlashCompute", "sd-xl removed from MegaNode"],
    circuit_breaker: ["Circuit breaker triggered for provider prov-3", "Error rate spike detected on prov-7", "Auto-failover activated for prov-2"],
    governance: ["New pricing proposal submitted", "Fee adjustment vote passed", "Provider slashing vote initiated"],
  };

  const now = Date.now();
  const events: EventItem[] = Array.from({ length: 20 }, (_, i) => {
    const type = eventTypes[Math.floor(Math.random() * eventTypes.length)];
    const messages = eventMessages[type];
    return {
      id: `evt-${i}`,
      type,
      message: messages[Math.floor(Math.random() * messages.length)],
      timestamp: new Date(now - i * 300000).toISOString(),
    };
  });

  return {
    systemHealth: Math.random() > 0.15 ? (Math.random() > 0.3 ? "green" : "yellow") : "red",
    activeProviders: providers.filter((p) => p.status !== "offline").length,
    requestsPerMin: Math.floor(Math.random() * 300 + 200),
    latencyP50: Math.floor(Math.random() * 100 + 60),
    latencyP95: Math.floor(Math.random() * 200 + 150),
    latencyP99: Math.floor(Math.random() * 400 + 300),
    errorRate: +(Math.random() * 3 + 0.1).toFixed(2),
    revenueToday: +(Math.random() * 500 + 100).toFixed(1),
    revenueWeek: +(Math.random() * 3000 + 1000).toFixed(1),
    revenueMonth: +(Math.random() * 10000 + 5000).toFixed(1),
    providers,
    requestHistory,
    modelDistribution,
    events,
  };
}

// ---------------------------------------------------------------------------
// Icons (inline SVG)
// ---------------------------------------------------------------------------

function IconActivity() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>;
}
function IconServer() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><rect x="2" y="2" width="20" height="8" rx="2" /><rect x="2" y="14" width="20" height="8" rx="2" /><line x1="6" y1="6" x2="6.01" y2="6" /><line x1="6" y1="18" x2="6.01" y2="18" /></svg>;
}
function IconZap() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" /></svg>;
}
function IconClock() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>;
}
function IconAlertTriangle() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" /><line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></svg>;
}
function IconCoins() {
  return <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><path d="M16 8h-6a2 2 0 00-2 2v1a2 2 0 002 2h4a2 2 0 012 2v1a2 2 0 01-2 2H8" /><path d="M12 18V6" /></svg>;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

type SortKey = "name" | "region" | "status" | "healthScore" | "activeTasks" | "latencyP50" | "uptime";
type SortDir = "asc" | "desc";

export default function OperatorDashboard() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("healthScore");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [selectedProvider, setSelectedProvider] = useState<string | null>(null);

  const loadData = useCallback(() => {
    setData(generateData());
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 10000);
    return () => clearInterval(interval);
  }, [loadData]);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };

  if (!data) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-8 w-48 bg-surface-200 rounded" />
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="h-24 bg-surface-200 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  const sortedProviders = [...data.providers].sort((a, b) => {
    const aVal = a[sortKey];
    const bVal = b[sortKey];
    if (typeof aVal === "string" && typeof bVal === "string") {
      return sortDir === "asc" ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal);
    }
    return sortDir === "asc" ? (aVal as number) - (bVal as number) : (bVal as number) - (aVal as number);
  });

  const healthColor = data.systemHealth === "green" ? "bg-accent-500" : data.systemHealth === "yellow" ? "bg-yellow-500" : "bg-danger-500";
  const healthLabel = data.systemHealth === "green" ? "Healthy" : data.systemHealth === "yellow" ? "Degraded" : "Critical";

  const maxRequest = Math.max(...data.requestHistory, 1);

  const eventTypeColors: Record<EventItem["type"], string> = {
    provider_joined: "text-accent-600",
    provider_left: "text-danger-500",
    model_added: "text-brand-600",
    model_removed: "text-surface-800/40",
    circuit_breaker: "text-yellow-600",
    governance: "text-purple-600",
  };

  const eventTypeIcons: Record<EventItem["type"], string> = {
    provider_joined: "M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z",
    provider_left: "M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2",
    model_added: "M12 4v16m8-8H4",
    model_removed: "M20 12H4",
    circuit_breaker: "M13 10V3L4 14h7v7l9-11h-7z",
    governance: "M12 15a3 3 0 100-6 3 3 0 000 6z M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z",
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Operator Dashboard</h1>
          <p className="text-sm text-surface-800/50 mt-1">Real-time network overview and management.</p>
        </div>
        <div className="flex items-center gap-2">
          <span className={`h-2.5 w-2.5 rounded-full ${healthColor}`} />
          <span className="text-sm font-medium">{healthLabel}</span>
          <span className="text-xs text-surface-800/40 ml-1">Auto-refreshes every 10s</span>
        </div>
      </div>

      {/* Stats cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard icon={<IconServer />} label="Active Providers" value={String(data.activeProviders)} color="text-brand-600" />
        <StatCard icon={<IconZap />} label="Requests/min" value={String(data.requestsPerMin)} color="text-accent-600" />
        <StatCard icon={<IconClock />} label="Latency (p50/p95/p99)" value={`${data.latencyP50}/${data.latencyP95}/${data.latencyP99}ms`} color="text-surface-800" />
        <StatCard icon={<IconAlertTriangle />} label="Error Rate" value={`${data.errorRate}%`} color={data.errorRate > 2 ? "text-danger-500" : "text-accent-600"} />
        <StatCard icon={<IconCoins />} label="Revenue Today" value={`${data.revenueToday} ERG`} color="text-accent-600" />
        <StatCard icon={<IconCoins />} label="Revenue Week" value={`${data.revenueWeek} ERG`} color="text-accent-600" />
        <StatCard icon={<IconCoins />} label="Revenue Month" value={`${data.revenueMonth} ERG`} color="text-accent-600" />
        <StatCard icon={<IconActivity />} label="System Health" value={healthLabel} color={data.systemHealth === "green" ? "text-accent-600" : data.systemHealth === "yellow" ? "text-yellow-600" : "text-danger-500"} />
      </div>

      {/* Request metrics chart */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <h2 className="text-sm font-semibold text-surface-900 mb-4">Requests per Minute (Last 60 min)</h2>
        <div className="flex items-end gap-px h-32">
          {data.requestHistory.map((val, i) => (
            <div
              key={i}
              className="flex-1 bg-brand-500 rounded-t-sm min-w-[2px] transition-all"
              style={{ height: `${(val / maxRequest) * 100}%`, opacity: 0.5 + (i / data.requestHistory.length) * 0.5 }}
              title={`${60 - i} min ago: ${val} req/min`}
            />
          ))}
        </div>
        <div className="flex justify-between mt-2 text-xs text-surface-800/30">
          <span>60m ago</span>
          <span>30m ago</span>
          <span>Now</span>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Provider table */}
        <div className="lg:col-span-2 rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
          <div className="px-5 py-4 border-b border-surface-200 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-surface-900">Providers</h2>
            <span className="text-xs text-surface-800/40">{data.providers.length} total</span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 text-left">
                  {([
                    ["name", "Name"],
                    ["region", "Region"],
                    ["status", "Status"],
                    ["healthScore", "Health"],
                    ["activeTasks", "Tasks"],
                    ["latencyP50", "Latency"],
                    ["uptime", "Uptime"],
                  ] as [SortKey, string][]).map(([key, label]) => (
                    <th
                      key={key}
                      onClick={() => handleSort(key)}
                      className="px-4 py-2.5 text-xs font-medium text-surface-800/50 cursor-pointer hover:text-surface-900 whitespace-nowrap"
                    >
                      {label}
                      {sortKey === key && (
                        <span className="ml-1">{sortDir === "asc" ? "\u2191" : "\u2193"}</span>
                      )}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {sortedProviders.map((p) => (
                  <tr
                    key={p.id}
                    className="border-b border-surface-100 hover:bg-surface-50 cursor-pointer transition-colors"
                    onClick={() => setSelectedProvider(selectedProvider === p.id ? null : p.id)}
                  >
                    <td className="px-4 py-3 font-medium">
                      <Link href={`/operator/providers/${p.id}`} className="hover:text-brand-600 transition-colors">
                        {p.name}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-surface-800/60">{p.region}</td>
                    <td className="px-4 py-3">
                      <StatusBadge status={p.status} />
                    </td>
                    <td className="px-4 py-3">
                      <HealthBar score={p.healthScore} />
                    </td>
                    <td className="px-4 py-3 text-surface-800/60">{p.activeTasks}</td>
                    <td className="px-4 py-3 text-surface-800/60">{p.latencyP50}ms</td>
                    <td className="px-4 py-3 text-surface-800/60">{p.uptime}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        {/* Right column */}
        <div className="space-y-6">
          {/* Model distribution */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h2 className="text-sm font-semibold text-surface-900 mb-4">Model Distribution</h2>
            <div className="space-y-3">
              {data.modelDistribution.slice(0, 6).map((m) => (
                <div key={m.model}>
                  <div className="flex items-center justify-between text-xs mb-1">
                    <span className="font-mono text-surface-800/70 truncate mr-2">{m.model}</span>
                    <span className="text-surface-800/40 flex-shrink-0">{m.percentage}%</span>
                  </div>
                  <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-brand-500"
                      style={{ width: `${m.percentage}%` }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Recent events */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h2 className="text-sm font-semibold text-surface-900 mb-4">Recent Events</h2>
            <div className="space-y-3 max-h-80 overflow-y-auto">
              {data.events.map((evt) => (
                <div key={evt.id} className="flex gap-2.5 text-xs">
                  <svg className={`w-4 h-4 flex-shrink-0 mt-0.5 ${eventTypeColors[evt.type]}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                    <path d={eventTypeIcons[evt.type]} />
                  </svg>
                  <div className="min-w-0">
                    <p className="text-surface-800/70 leading-relaxed">{evt.message}</p>
                    <p className="text-surface-800/30 mt-0.5">
                      {new Date(evt.timestamp).toLocaleTimeString()}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function StatCard({ icon, label, value, color }: { icon: React.ReactNode; label: string; value: string; color: string }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <div className={`flex items-center gap-2 text-xs font-medium text-surface-800/50 mb-2 ${color}`}>
        {icon}
        {label}
      </div>
      <p className="text-lg font-bold text-surface-900">{value}</p>
    </div>
  );
}

function StatusBadge({ status }: { status: Provider["status"] }) {
  const colors = {
    online: "bg-accent-100 text-accent-700",
    degraded: "bg-yellow-100 text-yellow-700",
    offline: "bg-surface-200 text-surface-800/40",
  };
  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${colors[status]}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${status === "online" ? "bg-accent-500" : status === "degraded" ? "bg-yellow-500" : "bg-surface-400"}`} />
      {status}
    </span>
  );
}

function HealthBar({ score }: { score: number }) {
  const color = score >= 90 ? "bg-accent-500" : score >= 70 ? "bg-yellow-500" : "bg-danger-500";
  return (
    <div className="flex items-center gap-2">
      <div className="w-16 h-1.5 rounded-full bg-surface-200 overflow-hidden">
        <div className={`h-full rounded-full ${color}`} style={{ width: `${score}%` }} />
      </div>
      <span className="text-xs text-surface-800/60">{score}</span>
    </div>
  );
}
