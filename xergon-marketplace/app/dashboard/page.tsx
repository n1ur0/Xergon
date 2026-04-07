"use client";

import { useState, useEffect, useCallback } from "react";
import { useAuth } from "@/lib/auth-context";
import { RELAY_BASE } from "@/lib/api/config";
import {
  DollarSign,
  Activity,
  Zap,
  Key,
  BarChart3,
  Copy,
  Plus,
  Trash2,
  RefreshCw,
  AlertCircle,
  TrendingUp,
  TrendingDown,
  Clock,
  Cpu,
  X,
} from "lucide-react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface UsageStats {
  totalSpendNanoErg: number;
  totalRequests: number;
  totalTokens: number;
  activeKeys: number;
  spendChange24h: number;
  requestsChange24h: number;
  dailyUsage: Array<{
    date: string;
    requests: number;
    tokens: number;
    spendNanoErg: number;
  }>;
}

interface RecentRequest {
  id: string;
  timestamp: string;
  model: string;
  tokensUsed: number;
  costNanoErg: number;
  latencyMs: number;
  status: "success" | "error" | "timeout";
}

interface ApiKey {
  id: string;
  key: string;
  name: string;
  createdAt: string;
  lastUsed: string | null;
  requestCount: number;
  status: "active" | "revoked";
}

interface ModelUsage {
  model: string;
  totalRequests: number;
  totalTokens: number;
  totalCostNanoErg: number;
  avgLatencyMs: number;
}

type ChartView = "requests" | "tokens" | "spend";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  if (erg >= 1_000) return `${(erg / 1_000).toFixed(1)}K ERG`;
  return `${erg.toFixed(4)} ERG`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function maskKey(key: string): string {
  if (key.length <= 12) return key;
  return `${key.slice(0, 8)}${"•".repeat(16)}${key.slice(-4)}`;
}

function timeAgo(dateStr: string): string {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diff = now - then;
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(dateStr).toLocaleDateString();
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function formatShortDate(dateStr: string): string {
  const d = new Date(dateStr);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

// ---------------------------------------------------------------------------
// Skeleton components
// ---------------------------------------------------------------------------

function SkeletonCard() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-5 animate-pulse">
      <div className="flex items-center justify-between mb-3">
        <div className="h-5 w-5 rounded bg-surface-200 dark:bg-surface-700" />
        <div className="h-4 w-16 rounded-full bg-surface-200 dark:bg-surface-700" />
      </div>
      <div className="h-7 w-28 rounded bg-surface-200 dark:bg-surface-700 mb-2" />
      <div className="h-3 w-20 rounded bg-surface-200 dark:bg-surface-700" />
    </div>
  );
}

function SkeletonChart() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
      <div className="flex items-center justify-between mb-6">
        <div className="h-5 w-32 rounded bg-surface-200 dark:bg-surface-700" />
        <div className="flex gap-2">
          <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
          <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
          <div className="h-8 w-20 rounded-lg bg-surface-200 dark:bg-surface-700" />
        </div>
      </div>
      <div className="flex items-end gap-1 h-48">
        {Array.from({ length: 30 }).map((_, i) => (
          <div key={i} className="flex-1 rounded-t bg-surface-200 dark:bg-surface-700" style={{ height: `${Math.random() * 80 + 10}%` }} />
        ))}
      </div>
    </div>
  );
}

function SkeletonTable({ rows = 5 }: { rows?: number }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
      <div className="h-5 w-40 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
      <div className="space-y-3">
        {Array.from({ length: rows }).map((_, i) => (
          <div key={i} className="h-10 w-full rounded-lg bg-surface-200 dark:bg-surface-700" />
        ))}
      </div>
    </div>
  );
}

function SkeletonActivity() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6 animate-pulse">
      <div className="h-5 w-32 rounded bg-surface-200 dark:bg-surface-700 mb-4" />
      <div className="space-y-3">
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="flex gap-3">
            <div className="h-8 w-8 rounded-full bg-surface-200 dark:bg-surface-700 shrink-0" />
            <div className="flex-1 space-y-1.5">
              <div className="h-3.5 w-3/4 rounded bg-surface-200 dark:bg-surface-700" />
              <div className="h-3 w-1/2 rounded bg-surface-200 dark:bg-surface-700" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Error section
// ---------------------------------------------------------------------------

function ErrorSection({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-12 text-center">
      <AlertCircle className="h-8 w-8 text-red-400" />
      <p className="text-sm text-surface-800/60 dark:text-surface-300/60">{message}</p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-3 py-1.5 text-sm transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
      >
        <RefreshCw className="h-3.5 w-3.5" />
        Retry
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: "success" | "error" | "timeout" | "active" | "revoked" }) {
  const styles: Record<string, string> = {
    success: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300",
    error: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300",
    timeout: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300",
    active: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300",
    revoked: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300",
  };
  return (
    <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${styles[status] || ""}`}>
      {status}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Main Dashboard Page
// ---------------------------------------------------------------------------

export default function DashboardPage() {
  const { isAuthenticated, publicKey } = useAuth();

  // Data state
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [recentActivity, setRecentActivity] = useState<RecentRequest[]>([]);
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
  const [modelUsage, setModelUsage] = useState<ModelUsage[]>([]);

  // UI state
  const [loading, setLoading] = useState(true);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [activityError, setActivityError] = useState<string | null>(null);
  const [keysError, setKeysError] = useState<string | null>(null);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [chartView, setChartView] = useState<ChartView>("requests");
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newKeyName, setNewKeyName] = useState("");
  const [creatingKey, setCreatingKey] = useState(false);
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [confirmRevokeId, setConfirmRevokeId] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // ---------------------------------------------------------------------------
  // Data fetching
  // ---------------------------------------------------------------------------

  const fetchStats = useCallback(async () => {
    setStatsError(null);
    try {
      const res = await fetch(`${RELAY_BASE}/usage/stats?period=30d`, {
        headers: publicKey ? { "X-Api-Key": publicKey } : {},
      });
      if (res.ok) {
        const data = await res.json();
        setStats(data);
      } else {
        setStatsError("Failed to load usage statistics");
      }
    } catch {
      setStatsError("Could not reach the relay server");
    }
  }, [publicKey]);

  const fetchActivity = useCallback(async () => {
    setActivityError(null);
    try {
      const res = await fetch(`${RELAY_BASE}/usage/recent?limit=10`, {
        headers: publicKey ? { "X-Api-Key": publicKey } : {},
      });
      if (res.ok) {
        const data = await res.json();
        setRecentActivity(data);
      } else {
        setActivityError("Failed to load recent activity");
      }
    } catch {
      setActivityError("Could not reach the relay server");
    }
  }, [publicKey]);

  const fetchKeys = useCallback(async () => {
    setKeysError(null);
    try {
      const res = await fetch(`${RELAY_BASE}/keys`, {
        headers: publicKey ? { "X-Api-Key": publicKey } : {},
      });
      if (res.ok) {
        const data = await res.json();
        setApiKeys(Array.isArray(data) ? data : data.keys ?? []);
      } else {
        setKeysError("Failed to load API keys");
      }
    } catch {
      setKeysError("Could not reach the relay server");
    }
  }, [publicKey]);

  const fetchModels = useCallback(async () => {
    setModelsError(null);
    try {
      const res = await fetch(`${RELAY_BASE}/usage/by-model`, {
        headers: publicKey ? { "X-Api-Key": publicKey } : {},
      });
      if (res.ok) {
        const data = await res.json();
        setModelUsage(Array.isArray(data) ? data : data.models ?? []);
      } else {
        setModelsError("Failed to load model usage");
      }
    } catch {
      setModelsError("Could not reach the relay server");
    }
  }, [publicKey]);

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    const load = async () => {
      setLoading(true);
      await Promise.all([fetchStats(), fetchActivity(), fetchKeys(), fetchModels()]);
      setLoading(false);
    };
    load();
  }, [isAuthenticated, fetchStats, fetchActivity, fetchKeys, fetchModels]);

  // ---------------------------------------------------------------------------
  // API Key actions
  // ---------------------------------------------------------------------------

  const handleCreateKey = async () => {
    if (!newKeyName.trim()) return;
    setCreatingKey(true);
    try {
      const res = await fetch(`${RELAY_BASE}/keys`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(publicKey ? { "X-Api-Key": publicKey } : {}),
        },
        body: JSON.stringify({ name: newKeyName.trim() }),
      });
      if (res.ok) {
        const data = await res.json();
        setCreatedKey(data.key ?? data.apiKey ?? data);
        setNewKeyName("");
        fetchKeys();
        fetchStats();
      }
    } catch {
      // silently fail
    } finally {
      setCreatingKey(false);
    }
  };

  const handleRevokeKey = async (id: string) => {
    setRevokingId(id);
    try {
      const res = await fetch(`${RELAY_BASE}/keys/${id}`, {
        method: "DELETE",
        headers: publicKey ? { "X-Api-Key": publicKey } : {},
      });
      if (res.ok) {
        setApiKeys((prev) => prev.map((k) => (k.id === id ? { ...k, status: "revoked" as const } : k)));
        fetchStats();
      }
    } catch {
      // silently fail
    } finally {
      setRevokingId(null);
      setConfirmRevokeId(null);
    }
  };

  const handleCopyKey = async (key: string, id: string) => {
    try {
      await navigator.clipboard.writeText(key);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch {
      // silently fail
    }
  };

  // ---------------------------------------------------------------------------
  // Auth guard
  // ---------------------------------------------------------------------------

  if (!isAuthenticated) {
    return (
      <div className="mx-auto max-w-6xl px-4 py-16 text-center">
        <Key className="mx-auto mb-4 h-12 w-12 text-surface-800/20 dark:text-surface-300/20" />
        <h1 className="text-xl font-bold text-surface-900 dark:text-surface-0 mb-2">Dashboard</h1>
        <p className="text-sm text-surface-800/60 dark:text-surface-300/60">
          Connect your wallet to view usage analytics, spend tracking, and manage API keys.
        </p>
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Loading state
  // ---------------------------------------------------------------------------

  if (loading) {
    return (
      <div className="mx-auto max-w-6xl px-4 py-8 space-y-6">
        <div className="h-8 w-48 rounded bg-surface-200 dark:bg-surface-700 animate-pulse" />
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <SkeletonCard key={i} />
          ))}
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2">
            <SkeletonChart />
          </div>
          <SkeletonActivity />
        </div>
        <SkeletonTable />
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  const chartData = stats?.dailyUsage ?? [];
  const maxChartValue = Math.max(...chartData.map((d) => {
    if (chartView === "requests") return d.requests;
    if (chartView === "tokens") return d.tokens;
    return d.spendNanoErg;
  }), 1);

  return (
    <div className="mx-auto max-w-6xl px-4 py-8 space-y-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-0">Dashboard</h1>
          <p className="text-sm text-surface-800/60 dark:text-surface-300/60">
            Usage analytics, spend tracking, and API key management
          </p>
        </div>
        <button
          onClick={() => Promise.all([fetchStats(), fetchActivity(), fetchKeys(), fetchModels()])}
          className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-3 py-2 text-sm transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </button>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* 1. Overview Cards */}
      {/* ------------------------------------------------------------------ */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Total Spend */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 dark:border-surface-700 dark:bg-surface-900">
          <div className="flex items-center justify-between mb-3">
            <DollarSign className="h-5 w-5 text-emerald-500" />
            {stats && (
              <span className={`inline-flex items-center gap-0.5 rounded-full px-2 py-0.5 text-xs font-medium ${
                stats.spendChange24h >= 0
                  ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"
                  : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300"
              }`}>
                {stats.spendChange24h >= 0 ? <TrendingUp className="h-3 w-3" /> : <TrendingDown className="h-3 w-3" />}
                {Math.abs(stats.spendChange24h).toFixed(1)}%
              </span>
            )}
          </div>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">
            {stats ? nanoergToErg(stats.totalSpendNanoErg) : "--"}
          </p>
          <p className="text-xs text-surface-800/50 dark:text-surface-300/50">Total Spend</p>
        </div>

        {/* Total Requests */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 dark:border-surface-700 dark:bg-surface-900">
          <div className="flex items-center justify-between mb-3">
            <Activity className="h-5 w-5 text-blue-500" />
            {stats && (
              <span className={`inline-flex items-center gap-0.5 rounded-full px-2 py-0.5 text-xs font-medium ${
                stats.requestsChange24h >= 0
                  ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"
                  : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300"
              }`}>
                {stats.requestsChange24h >= 0 ? <TrendingUp className="h-3 w-3" /> : <TrendingDown className="h-3 w-3" />}
                {Math.abs(stats.requestsChange24h).toFixed(1)}%
              </span>
            )}
          </div>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">
            {stats ? formatNumber(stats.totalRequests) : "--"}
          </p>
          <p className="text-xs text-surface-800/50 dark:text-surface-300/50">Total Requests</p>
        </div>

        {/* Tokens Consumed */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 dark:border-surface-700 dark:bg-surface-900">
          <div className="flex items-center justify-between mb-3">
            <Zap className="h-5 w-5 text-amber-500" />
            <span className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-xs font-medium text-surface-800/50 dark:bg-surface-800 dark:text-surface-300/50">
              24h
            </span>
          </div>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">
            {stats ? formatNumber(stats.totalTokens) : "--"}
          </p>
          <p className="text-xs text-surface-800/50 dark:text-surface-300/50">Tokens Consumed</p>
        </div>

        {/* Active API Keys */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 dark:border-surface-700 dark:bg-surface-900">
          <div className="flex items-center justify-between mb-3">
            <Key className="h-5 w-5 text-purple-500" />
            <span className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-xs font-medium text-surface-800/50 dark:bg-surface-800 dark:text-surface-300/50">
              active
            </span>
          </div>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">
            {stats ? String(stats.activeKeys) : "--"}
          </p>
          <p className="text-xs text-surface-800/50 dark:text-surface-300/50">API Keys</p>
        </div>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* 2. Usage Chart + 3. Recent Activity */}
      {/* ------------------------------------------------------------------ */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Usage Chart */}
        <div className="lg:col-span-2 rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
          <div className="flex items-center justify-between mb-6">
            <h2 className="flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider">
              <BarChart3 className="h-4 w-4" />
              Usage (Last 30 Days)
            </h2>
            <div className="flex gap-1 rounded-lg bg-surface-100 p-1 dark:bg-surface-800">
              {(["requests", "tokens", "spend"] as ChartView[]).map((view) => (
                <button
                  key={view}
                  onClick={() => setChartView(view)}
                  className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                    chartView === view
                      ? "bg-white text-surface-900 shadow-sm dark:bg-surface-700 dark:text-surface-0"
                      : "text-surface-800/50 hover:text-surface-800/70 dark:text-surface-300/50 dark:hover:text-surface-300/70"
                  }`}
                >
                  {view.charAt(0).toUpperCase() + view.slice(1)}
                </button>
              ))}
            </div>
          </div>

          {statsError ? (
            <ErrorSection message={statsError} onRetry={fetchStats} />
          ) : chartData.length === 0 ? (
            <div className="flex items-center justify-center h-48 text-sm text-surface-800/40 dark:text-surface-300/40">
              No usage data available yet
            </div>
          ) : (
            <div className="flex items-end gap-[3px] h-48">
              {chartData.map((d, i) => {
                let value: number;
                let label: string;
                if (chartView === "requests") {
                  value = d.requests;
                  label = formatNumber(value);
                } else if (chartView === "tokens") {
                  value = d.tokens;
                  label = formatNumber(value);
                } else {
                  value = d.spendNanoErg;
                  label = nanoergToErg(value);
                }
                const pct = (value / maxChartValue) * 100;
                return (
                  <div
                    key={i}
                    className="flex-1 flex flex-col items-center gap-1 group"
                    title={`${formatShortDate(d.date)}: ${label}`}
                  >
                    {/* Tooltip on hover */}
                    <div className="hidden group-hover:block absolute -top-10 z-10 rounded-md bg-surface-900 px-2 py-1 text-xs text-white whitespace-nowrap dark:bg-surface-0 dark:text-surface-900">
                      {label}
                    </div>
                    <div
                      className="w-full rounded-t bg-brand-500/80 transition-all duration-150 hover:bg-brand-600 min-h-[2px]"
                      style={{ height: `${Math.max(pct, 2)}%` }}
                    />
                    {/* Show date labels every 5 days */}
                    {(i === 0 || i === chartData.length - 1 || (i + 1) % 5 === 0) && (
                      <span className="text-[9px] text-surface-800/30 dark:text-surface-300/30 shrink-0 mt-1">
                        {formatShortDate(d.date)}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Recent Activity */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
          <h2 className="flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider mb-4">
            <Clock className="h-4 w-4" />
            Recent Activity
          </h2>

          {activityError ? (
            <ErrorSection message={activityError} onRetry={fetchActivity} />
          ) : recentActivity.length === 0 ? (
            <div className="flex items-center justify-center py-16 text-sm text-surface-800/40 dark:text-surface-300/40">
              No recent requests
            </div>
          ) : (
            <div className="space-y-3 max-h-[24rem] overflow-y-auto pr-1">
              {recentActivity.map((req) => (
                <div
                  key={req.id}
                  className="flex items-start gap-3 rounded-lg border border-surface-100 p-3 transition-colors hover:bg-surface-50 dark:border-surface-800 dark:hover:bg-surface-800/50"
                >
                  <div className={`mt-0.5 h-7 w-7 shrink-0 rounded-full flex items-center justify-center ${
                    req.status === "success"
                      ? "bg-emerald-100 dark:bg-emerald-900/30"
                      : req.status === "timeout"
                      ? "bg-amber-100 dark:bg-amber-900/30"
                      : "bg-red-100 dark:bg-red-900/30"
                  }`}>
                    {req.status === "success" ? (
                      <Activity className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
                    ) : (
                      <AlertCircle className="h-3.5 w-3.5 text-red-600 dark:text-red-400" />
                    )}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between gap-2">
                      <p className="text-sm font-medium text-surface-900 dark:text-surface-0 truncate">
                        {req.model}
                      </p>
                      <StatusBadge status={req.status} />
                    </div>
                    <div className="flex items-center gap-3 mt-1 text-xs text-surface-800/50 dark:text-surface-300/50">
                      <span>{timeAgo(req.timestamp)}</span>
                      <span>{formatNumber(req.tokensUsed)} tok</span>
                      <span>{nanoergToErg(req.costNanoErg)}</span>
                      <span>{req.latencyMs}ms</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* 4. API Key Management */}
      {/* ------------------------------------------------------------------ */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider">
            <Key className="h-4 w-4" />
            API Keys
          </h2>
          <button
            onClick={() => setShowCreateModal(true)}
            className="inline-flex items-center gap-1.5 rounded-lg bg-brand-600 px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            <Plus className="h-3.5 w-3.5" />
            Create New Key
          </button>
        </div>

        {keysError ? (
          <ErrorSection message={keysError} onRetry={fetchKeys} />
        ) : apiKeys.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <Key className="mb-3 h-8 w-8 text-surface-800/20 dark:text-surface-300/20" />
            <p className="text-sm text-surface-800/50 dark:text-surface-300/50">No API keys yet</p>
            <p className="text-xs text-surface-800/40 dark:text-surface-300/40 mt-1">
              Create your first API key to start using the Xergon relay
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 dark:border-surface-700">
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50">Name</th>
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50">Key</th>
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50 hidden sm:table-cell">Created</th>
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50 hidden md:table-cell">Last Used</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50 hidden sm:table-cell">Requests</th>
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50">Status</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-100 dark:divide-surface-800">
                {apiKeys.map((apiKey) => (
                  <tr key={apiKey.id} className="group">
                    <td className="py-3 pr-4 font-medium text-surface-900 dark:text-surface-0">
                      {apiKey.name}
                    </td>
                    <td className="py-3 pr-4">
                      <code className="rounded bg-surface-100 px-2 py-0.5 text-xs font-mono text-surface-800/70 dark:bg-surface-800 dark:text-surface-300/70">
                        {maskKey(apiKey.key)}
                      </code>
                    </td>
                    <td className="py-3 pr-4 text-surface-800/50 dark:text-surface-300/50 hidden sm:table-cell whitespace-nowrap">
                      {formatDate(apiKey.createdAt)}
                    </td>
                    <td className="py-3 pr-4 text-surface-800/50 dark:text-surface-300/50 hidden md:table-cell whitespace-nowrap">
                      {apiKey.lastUsed ? timeAgo(apiKey.lastUsed) : "Never"}
                    </td>
                    <td className="py-3 pr-4 text-right text-surface-800/70 dark:text-surface-300/70 hidden sm:table-cell">
                      {formatNumber(apiKey.requestCount)}
                    </td>
                    <td className="py-3 pr-4">
                      <StatusBadge status={apiKey.status} />
                    </td>
                    <td className="py-3 text-right">
                      <div className="flex items-center justify-end gap-1">
                        <button
                          onClick={() => handleCopyKey(apiKey.key, apiKey.id)}
                          className="inline-flex items-center justify-center rounded-md p-1.5 text-surface-800/40 transition-colors hover:bg-surface-100 hover:text-surface-800/70 dark:text-surface-300/40 dark:hover:bg-surface-800 dark:hover:text-surface-300/70"
                          title="Copy key"
                        >
                          {copiedId === apiKey.id ? (
                            <span className="text-xs text-emerald-600 dark:text-emerald-400">Copied!</span>
                          ) : (
                            <Copy className="h-3.5 w-3.5" />
                          )}
                        </button>
                        {apiKey.status === "active" && (
                          confirmRevokeId === apiKey.id ? (
                            <div className="flex items-center gap-1">
                              <button
                                onClick={() => handleRevokeKey(apiKey.id)}
                                disabled={revokingId === apiKey.id}
                                className="inline-flex items-center rounded-md bg-red-600 px-2 py-1 text-xs font-medium text-white transition-colors hover:bg-red-700 disabled:opacity-50"
                              >
                                {revokingId === apiKey.id ? "..." : "Confirm"}
                              </button>
                              <button
                                onClick={() => setConfirmRevokeId(null)}
                                className="inline-flex items-center justify-center rounded-md p-1 text-surface-800/40 transition-colors hover:text-surface-800/70 dark:text-surface-300/40 dark:hover:text-surface-300/70"
                              >
                                <X className="h-3.5 w-3.5" />
                              </button>
                            </div>
                          ) : (
                            <button
                              onClick={() => setConfirmRevokeId(apiKey.id)}
                              className="inline-flex items-center justify-center rounded-md p-1.5 text-surface-800/40 transition-colors hover:bg-red-50 hover:text-red-600 dark:text-surface-300/40 dark:hover:bg-red-900/20 dark:hover:text-red-400"
                              title="Revoke key"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          )
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* 5. Model Usage Breakdown */}
      {/* ------------------------------------------------------------------ */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 dark:border-surface-700 dark:bg-surface-900 p-6">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-surface-800/60 uppercase tracking-wider mb-4">
          <Cpu className="h-4 w-4" />
          Model Usage Breakdown
        </h2>

        {modelsError ? (
          <ErrorSection message={modelsError} onRetry={fetchModels} />
        ) : modelUsage.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <Cpu className="mb-3 h-8 w-8 text-surface-800/20 dark:text-surface-300/20" />
            <p className="text-sm text-surface-800/50 dark:text-surface-300/50">No model usage data yet</p>
            <p className="text-xs text-surface-800/40 dark:text-surface-300/40 mt-1">
              Start making requests to see per-model breakdown
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 dark:border-surface-700">
                  <th className="pb-3 text-left font-medium text-surface-800/50 dark:text-surface-300/50">Model</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50">Requests</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50 hidden sm:table-cell">Tokens</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50">Cost</th>
                  <th className="pb-3 text-right font-medium text-surface-800/50 dark:text-surface-300/50 hidden sm:table-cell">Avg Latency</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-100 dark:divide-surface-800">
                {modelUsage.map((m) => (
                  <tr key={m.model} className="hover:bg-surface-50 dark:hover:bg-surface-800/50">
                    <td className="py-3 pr-4 font-medium text-surface-900 dark:text-surface-0">
                      <div className="flex items-center gap-2">
                        <Cpu className="h-4 w-4 text-brand-500 shrink-0" />
                        <span className="truncate max-w-[200px]">{m.model}</span>
                      </div>
                    </td>
                    <td className="py-3 pr-4 text-right text-surface-800/70 dark:text-surface-300/70">
                      {formatNumber(m.totalRequests)}
                    </td>
                    <td className="py-3 pr-4 text-right text-surface-800/70 dark:text-surface-300/70 hidden sm:table-cell">
                      {formatNumber(m.totalTokens)}
                    </td>
                    <td className="py-3 pr-4 text-right font-medium text-surface-900 dark:text-surface-0">
                      {nanoergToErg(m.totalCostNanoErg)}
                    </td>
                    <td className="py-3 text-right text-surface-800/70 dark:text-surface-300/70 hidden sm:table-cell">
                      <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${
                        m.avgLatencyMs < 500
                          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300"
                          : m.avgLatencyMs < 2000
                          ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300"
                          : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300"
                      }`}>
                        {m.avgLatencyMs}ms
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ------------------------------------------------------------------ */}
      {/* Create Key Modal */}
      {/* ------------------------------------------------------------------ */}
      {showCreateModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
          <div className="w-full max-w-md rounded-2xl border border-surface-200 bg-white p-6 dark:border-surface-700 dark:bg-surface-900 shadow-xl">
            {createdKey ? (
              <>
                <div className="text-center mb-4">
                  <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-emerald-100 dark:bg-emerald-900/30">
                    <Key className="h-6 w-6 text-emerald-600 dark:text-emerald-400" />
                  </div>
                  <h3 className="text-lg font-semibold text-surface-900 dark:text-surface-0">API Key Created</h3>
                  <p className="mt-1 text-sm text-surface-800/60 dark:text-surface-300/60">
                    Copy this key now. You won't be able to see it again.
                  </p>
                </div>
                <div className="rounded-lg border border-surface-200 bg-surface-50 p-3 dark:border-surface-700 dark:bg-surface-800">
                  <code className="block break-all text-xs font-mono text-surface-900 dark:text-surface-0">
                    {createdKey}
                  </code>
                </div>
                <div className="flex gap-3 mt-4">
                  <button
                    onClick={() => handleCopyKey(createdKey, "new")}
                    className="flex-1 inline-flex items-center justify-center gap-1.5 rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700"
                  >
                    <Copy className="h-4 w-4" />
                    {copiedId === "new" ? "Copied!" : "Copy Key"}
                  </button>
                  <button
                    onClick={() => {
                      setShowCreateModal(false);
                      setCreatedKey(null);
                    }}
                    className="flex-1 rounded-lg border border-surface-300 px-4 py-2.5 text-sm font-medium transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-lg font-semibold text-surface-900 dark:text-surface-0">Create API Key</h3>
                  <button
                    onClick={() => setShowCreateModal(false)}
                    className="rounded-md p-1 text-surface-800/40 transition-colors hover:text-surface-800/70 dark:text-surface-300/40 dark:hover:text-surface-300/70"
                  >
                    <X className="h-5 w-5" />
                  </button>
                </div>
                <div className="space-y-4">
                  <div>
                    <label className="mb-1 block text-sm font-medium text-surface-800/70 dark:text-surface-300/70">
                      Key Name
                    </label>
                    <input
                      type="text"
                      value={newKeyName}
                      onChange={(e) => setNewKeyName(e.target.value)}
                      placeholder="e.g., My App, Production, Testing"
                      className="w-full rounded-lg border border-surface-300 bg-surface-0 px-3 py-2 text-sm dark:border-surface-600 dark:bg-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/50 focus:border-brand-500"
                      onKeyDown={(e) => e.key === "Enter" && handleCreateKey()}
                      autoFocus
                    />
                  </div>
                  <p className="text-xs text-surface-800/40 dark:text-surface-300/40">
                    The full API key will be shown once after creation. Store it securely.
                  </p>
                </div>
                <div className="flex gap-3 mt-6">
                  <button
                    onClick={() => setShowCreateModal(false)}
                    className="flex-1 rounded-lg border border-surface-300 px-4 py-2.5 text-sm font-medium transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleCreateKey}
                    disabled={!newKeyName.trim() || creatingKey}
                    className="flex-1 inline-flex items-center justify-center gap-1.5 rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
                  >
                    {creatingKey ? (
                      <RefreshCw className="h-4 w-4 animate-spin" />
                    ) : (
                      <Plus className="h-4 w-4" />
                    )}
                    {creatingKey ? "Creating..." : "Create Key"}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
