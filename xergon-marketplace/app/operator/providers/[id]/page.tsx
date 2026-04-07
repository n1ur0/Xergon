"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { toast } from "sonner";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface HealthBreakdown {
  latency: number;
  reliability: number;
  availability: number;
  throughput: number;
  error_rate: number;
  reputation: number;
}

interface ModelEntry {
  name: string;
  pricePer1MTokens: number;
  available: boolean;
  requests24h: number;
  avgLatency: number;
}

interface LatencyPoint {
  time: string;
  value: number;
}

interface ProviderDetail {
  id: string;
  name: string;
  endpoint: string;
  region: string;
  status: "online" | "degraded" | "offline";
  uptime: number;
  healthScore: number;
  healthBreakdown: HealthBreakdown;
  totalRequests: number;
  avgLatency: number;
  errorRate: number;
  tokensServed: number;
  aiPointsEarned: number;
  models: ModelEntry[];
  latencyHistory: LatencyPoint[];
  gpuInfo?: string;
  ergoAddress?: string;
  lastSeen?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function nanoErgToErg(nano: number): string {
  return (nano / 1_000_000_000).toFixed(4);
}

function scoreColor(score: number): string {
  if (score >= 90) return "text-accent-600";
  if (score >= 70) return "text-yellow-600";
  return "text-danger-500";
}

function scoreBarColor(score: number): string {
  if (score >= 90) return "bg-accent-500";
  if (score >= 70) return "bg-yellow-400";
  return "bg-danger-400";
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function ProviderDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const [provider, setProvider] = useState<ProviderDetail | null>(null);
  const [providerId, setProviderId] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);

  const loadData = useCallback(async (id: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`/api/operator/providers/${encodeURIComponent(id)}`);
      if (!res.ok) {
        const errBody = await res.json().catch(() => ({}));
        throw new Error(errBody.error ?? `Provider not found (${res.status})`);
      }
      const data = await res.json();

      // Normalize the response into our ProviderDetail shape
      const detail: ProviderDetail = {
        id,
        name: data.name ?? `Provider ${id}`,
        endpoint: data.endpoint ?? id,
        region: data.region ?? "Unknown",
        status: data.status ?? "offline",
        uptime: data.uptime ?? 0,
        healthScore: data.healthScore ?? data.health_score ?? 0,
        healthBreakdown: {
          latency: data.healthBreakdown?.latency ?? data.health_breakdown?.latency ?? 0,
          reliability: data.healthBreakdown?.reliability ?? data.health_breakdown?.reliability ?? 0,
          availability: data.healthBreakdown?.availability ?? data.health_breakdown?.availability ?? 0,
          throughput: data.healthBreakdown?.throughput ?? data.health_breakdown?.throughput ?? 0,
          error_rate: data.healthBreakdown?.error_rate ?? data.health_breakdown?.error_rate ?? 0,
          reputation: data.healthBreakdown?.reputation ?? data.health_breakdown?.reputation ?? 0,
        },
        totalRequests: data.totalRequests ?? data.total_requests ?? 0,
        avgLatency: data.avgLatency ?? data.avg_latency ?? data.latencyMs ?? 0,
        errorRate: data.errorRate ?? data.error_rate ?? 0,
        tokensServed: data.tokensServed ?? data.tokensServed ?? data.tokens_served ?? data.totalTokens ?? 0,
        aiPointsEarned: data.aiPointsEarned ?? data.ai_points_earned ?? data.aiPoints ?? 0,
        models: (data.models ?? []).map((m: string | { name: string; [k: string]: unknown }) =>
          typeof m === "string"
            ? { name: m, pricePer1MTokens: 0, available: true, requests24h: 0, avgLatency: 0 }
            : {
                name: m.name ?? "unknown",
                pricePer1MTokens: m.pricePer1MTokens ?? m.price_per_million ?? 0,
                available: m.available !== undefined ? m.available : m.status === "active",
                requests24h: m.requests24h ?? m.requests_24h ?? 0,
                avgLatency: m.avgLatency ?? m.avg_latency ?? 0,
              }
        ),
        latencyHistory: data.latencyHistory ?? data.latency_history ?? [],
        gpuInfo: data.gpuInfo ?? data.gpu_info,
        ergoAddress: data.ergoAddress ?? data.ergo_address,
        lastSeen: data.lastSeen ?? data.last_seen,
      };

      setProvider(detail);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    params.then((p) => {
      setProviderId(p.id);
      loadData(p.id);
    });
  }, [params, loadData]);

  // Auto-refresh every 30s
  useEffect(() => {
    if (!providerId) return;
    const interval = setInterval(() => loadData(providerId), 30_000);
    return () => clearInterval(interval);
  }, [providerId, loadData]);

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  const handleAction = async (action: "pause" | "resume" | "remove") => {
    setActionLoading(true);
    try {
      const res = await fetch(`/api/operator/providers/${encodeURIComponent(providerId)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action }),
      });
      if (!res.ok) {
        const errBody = await res.json().catch(() => ({}));
        throw new Error(errBody.error ?? `Action failed (${res.status})`);
      }
      toast.success(`Provider ${action}d successfully`);
      loadData(providerId);
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setActionLoading(false);
    }
  };

  // ---------------------------------------------------------------------------
  // Loading state
  // ---------------------------------------------------------------------------

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-8 w-48 bg-surface-200 rounded" />
        <div className="h-20 bg-surface-200 rounded-xl" />
        <div className="grid grid-cols-2 lg:grid-cols-6 gap-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-24 bg-surface-200 rounded-xl" />
          ))}
        </div>
        <div className="h-48 bg-surface-200 rounded-xl" />
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Error state
  // ---------------------------------------------------------------------------

  if (error || !provider) {
    return (
      <div className="space-y-6">
        <Link href="/operator/providers" className="inline-flex items-center gap-1 text-sm text-surface-800/50 hover:text-surface-900 transition-colors">
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="19" y1="12" x2="5" y2="12" /><polyline points="12 19 5 12 12 5" />
          </svg>
          Back to Providers
        </Link>
        <div className="rounded-xl border border-danger-200 bg-danger-50/50 p-8 text-center">
          <p className="text-sm text-danger-700 font-medium">Failed to load provider</p>
          <p className="text-xs text-danger-500 mt-1">{error ?? "Unknown error"}</p>
          <button
            onClick={() => loadData(providerId)}
            className="mt-4 text-sm text-danger-600 hover:underline font-medium"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  const statusColors: Record<string, string> = {
    online: "bg-accent-100 text-accent-700",
    degraded: "bg-yellow-100 text-yellow-700",
    offline: "bg-surface-200 text-surface-800/40",
  };

  const statusDot: Record<string, string> = {
    online: "bg-accent-500",
    degraded: "bg-yellow-500",
    offline: "bg-surface-400",
  };

  const hb = provider.healthBreakdown;
  const hasBreakdown = hb.latency > 0 || hb.reliability > 0 || hb.availability > 0 ||
    hb.throughput > 0 || hb.error_rate > 0 || hb.reputation > 0;

  const maxLatency = provider.latencyHistory.length > 0
    ? Math.max(...provider.latencyHistory.map((h) => h.value), 1)
    : 1;

  return (
    <div className="space-y-6">
      {/* Back button */}
      <div className="flex items-center gap-3">
        <Link href="/operator/providers" className="inline-flex items-center gap-1 text-sm text-surface-800/50 hover:text-surface-900 transition-colors">
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="19" y1="12" x2="5" y2="12" /><polyline points="12 19 5 12 12 5" />
          </svg>
          Back to Providers
        </Link>
      </div>

      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-bold text-surface-900">{provider.name}</h1>
            <span className={`inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-medium ${statusColors[provider.status] ?? statusColors.offline}`}>
              <span className={`h-1.5 w-1.5 rounded-full ${statusDot[provider.status] ?? statusDot.offline}`} />
              {provider.status}
            </span>
          </div>
          <p className="text-sm text-surface-800/50 mt-1">
            {provider.region}
            {provider.gpuInfo && <span className="mx-1.5">&middot;</span>}
            {provider.gpuInfo}
            {provider.uptime > 0 && (
              <>
                <span className="mx-1.5">&middot;</span>
                {provider.uptime}% uptime
              </>
            )}
          </p>
          {provider.endpoint && (
            <p className="text-xs text-surface-800/30 font-mono mt-0.5 truncate max-w-[500px]">{provider.endpoint}</p>
          )}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2 flex-wrap">
          {provider.status !== "offline" && (
            <ActionButton
              icon={
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <rect x="6" y="4" width="4" height="16" /><rect x="14" y="4" width="4" height="16" />
                </svg>
              }
              label="Pause"
              variant="outline"
              onClick={() => handleAction("pause")}
              disabled={actionLoading}
            />
          )}
          {provider.status !== "online" && (
            <ActionButton
              icon={
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <polygon points="5 3 19 12 5 21 5 3" />
                </svg>
              }
              label="Resume"
              variant="primary"
              onClick={() => handleAction("resume")}
              disabled={actionLoading}
            />
          )}
          <ActionButton
            icon={
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
              </svg>
            }
            label="Remove"
            variant="danger"
            onClick={() => {
              if (confirm("Are you sure you want to remove this provider?")) {
                handleAction("remove");
              }
            }}
            disabled={actionLoading}
          />
        </div>
      </div>

      {/* Health Score with Breakdown */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Large health score */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 flex flex-col items-center justify-center">
          <p className="text-xs text-surface-800/50 mb-2">Health Score</p>
          <div className={`text-5xl font-bold ${scoreColor(provider.healthScore)}`}>
            {provider.healthScore}
          </div>
          <p className="text-xs text-surface-800/30 mt-1">out of 100</p>
        </div>

        {/* Health breakdown bars */}
        <div className="lg:col-span-2 rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="text-sm font-semibold text-surface-900 mb-4">Health Breakdown</h2>
          {hasBreakdown ? (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-4">
              <ScoreBar label="Latency" score={hb.latency} />
              <ScoreBar label="Reliability" score={hb.reliability} />
              <ScoreBar label="Availability" score={hb.availability} />
              <ScoreBar label="Throughput" score={hb.throughput} />
              <ScoreBar label="Error Rate" score={hb.error_rate} invert />
              <ScoreBar label="Reputation" score={hb.reputation} />
            </div>
          ) : (
            <div className="flex items-center justify-center h-full text-sm text-surface-800/40 py-8">
              No detailed health data available from relay.
            </div>
          )}
        </div>
      </div>

      {/* Performance Metrics */}
      <div className="grid grid-cols-2 lg:grid-cols-5 gap-4">
        <MetricCard label="Total Requests" value={provider.totalRequests.toLocaleString()} subtext="all time" />
        <MetricCard label="Avg Latency" value={`${provider.avgLatency}ms`} subtext="current" />
        <MetricCard
          label="Error Rate"
          value={`${provider.errorRate}%`}
          subtext={provider.errorRate > 2 ? "above threshold" : "normal"}
          valueClass={provider.errorRate > 2 ? "text-danger-500" : undefined}
        />
        <MetricCard label="Tokens Served" value={formatTokens(provider.tokensServed)} subtext="total" />
        <MetricCard label="AI Points" value={provider.aiPointsEarned.toLocaleString()} subtext="earned" />
      </div>

      {/* Latency History */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <h2 className="text-sm font-semibold text-surface-900 mb-4">Latency History</h2>
        {provider.latencyHistory.length > 0 ? (
          <>
            <div className="flex items-end gap-px h-40">
              {provider.latencyHistory.map((point, i) => (
                <div
                  key={i}
                  className="flex-1 min-w-[3px] bg-brand-400 rounded-t-sm transition-all hover:bg-brand-500"
                  style={{ height: `${(point.value / maxLatency) * 100}%` }}
                  title={`${new Date(point.time).toLocaleTimeString()}: ${point.value}ms`}
                />
              ))}
            </div>
            <div className="flex justify-between mt-2 text-xs text-surface-800/30">
              {provider.latencyHistory.length > 0 && (
                <>
                  <span>{new Date(provider.latencyHistory[0].time).toLocaleTimeString()}</span>
                  <span>Now</span>
                </>
              )}
            </div>
          </>
        ) : (
          <div className="flex items-center justify-center h-24 text-sm text-surface-800/40">
            No latency history data available.
          </div>
        )}
      </div>

      {/* Models Table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-200">
          <h2 className="text-sm font-semibold text-surface-900">
            Models ({provider.models.length})
          </h2>
        </div>
        {provider.models.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 text-left bg-surface-50">
                  <th className="px-5 py-2.5 text-xs font-medium text-surface-800/50">Model</th>
                  <th className="px-5 py-2.5 text-xs font-medium text-surface-800/50">Price / 1M tok</th>
                  <th className="px-5 py-2.5 text-xs font-medium text-surface-800/50 hidden sm:table-cell">Requests (24h)</th>
                  <th className="px-5 py-2.5 text-xs font-medium text-surface-800/50 hidden md:table-cell">Avg Latency</th>
                  <th className="px-5 py-2.5 text-xs font-medium text-surface-800/50">Status</th>
                </tr>
              </thead>
              <tbody>
                {provider.models.map((m) => (
                  <tr key={m.name} className="border-b border-surface-100 hover:bg-surface-50 transition-colors">
                    <td className="px-5 py-3 font-mono text-xs">{m.name}</td>
                    <td className="px-5 py-3 text-surface-800/60">
                      {m.pricePer1MTokens > 0
                        ? `${nanoErgToErg(m.pricePer1MTokens)} ERG`
                        : "N/A"}
                    </td>
                    <td className="px-5 py-3 text-surface-800/60 hidden sm:table-cell">
                      {m.requests24h > 0 ? m.requests24h.toLocaleString() : "-"}
                    </td>
                    <td className="px-5 py-3 text-surface-800/60 hidden md:table-cell">
                      {m.avgLatency > 0 ? `${m.avgLatency}ms` : "-"}
                    </td>
                    <td className="px-5 py-3">
                      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${
                        m.available ? "bg-accent-100 text-accent-700" : "bg-surface-100 text-surface-800/40"
                      }`}>
                        <span className={`h-1.5 w-1.5 rounded-full ${m.available ? "bg-accent-500" : "bg-surface-400"}`} />
                        {m.available ? "available" : "unavailable"}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="px-5 py-8 text-center text-sm text-surface-800/40">
            No models listed for this provider.
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function ActionButton({
  icon,
  label,
  variant,
  onClick,
  disabled,
}: {
  icon: React.ReactNode;
  label: string;
  variant: "primary" | "outline" | "danger";
  onClick: () => void;
  disabled?: boolean;
}) {
  const base = "inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
  const variants = {
    primary: "bg-brand-600 text-white hover:bg-brand-700",
    outline: "border border-surface-200 text-surface-800/70 hover:bg-surface-100",
    danger: "border border-danger-200 text-danger-600 hover:bg-danger-50",
  };
  return (
    <button type="button" onClick={onClick} disabled={disabled} className={`${base} ${variants[variant]}`}>
      {icon}
      {label}
    </button>
  );
}

function MetricCard({
  label,
  value,
  subtext,
  valueClass,
}: {
  label: string;
  value: string;
  subtext: string;
  valueClass?: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <p className="text-xs text-surface-800/50 mb-1">{label}</p>
      <p className={`text-xl font-bold text-surface-900 ${valueClass ?? ""}`}>{value}</p>
      <p className="text-xs text-surface-800/30 mt-0.5">{subtext}</p>
    </div>
  );
}

function ScoreBar({
  label,
  score,
  invert = false,
}: {
  label: string;
  score: number;
  invert?: boolean;
}) {
  // For error_rate, lower is better, so invert the color
  const displayScore = Math.min(Math.max(score, 0), 100);
  const color = invert
    ? (displayScore <= 10 ? "bg-accent-500" : displayScore <= 30 ? "bg-yellow-400" : "bg-danger-400")
    : scoreBarColor(displayScore);

  return (
    <div>
      <div className="flex items-center justify-between text-sm mb-1">
        <span className="text-surface-800/60">{label}</span>
        <span className="font-medium text-surface-900">{displayScore.toFixed(1)}</span>
      </div>
      <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
        <div
          className={`h-full rounded-full ${color} transition-all`}
          style={{ width: `${displayScore}%` }}
        />
      </div>
    </div>
  );
}
