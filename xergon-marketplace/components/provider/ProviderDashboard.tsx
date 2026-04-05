"use client";

import { useState, useEffect, useCallback } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import {
  fetchProviderDashboardData,
  type ProviderDashboardData,
  type AiPointsModelBreakdown,
} from "@/lib/api/provider";
import { PricingSection } from "@/components/provider/PricingSection";
import { EarningsChart } from "@/components/provider/EarningsChart";
import { RequestHeatmap } from "@/components/provider/RequestHeatmap";
import { ModelPerformanceChart } from "@/components/provider/ModelPerformanceChart";
import { RealtimeMetrics } from "@/components/provider/RealtimeMetrics";
import { SkeletonMetricGrid } from "@/components/ui/SkeletonElements";
import { ApiErrorDisplay } from "@/components/ui/ErrorBoundary";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import Link from "next/link";

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toFixed(0);
}

function formatErg(nanoerg: number): string {
  const erg = nanoerg / 1e9;
  if (erg >= 1) return `${erg.toFixed(4)} ERG`;
  if (erg >= 0.001) return `${erg.toFixed(6)} ERG`;
  return `${nanoerg} nanoERG`;
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

function formatBytes(bytes: number): string {
  const gb = bytes / (1024 ** 3);
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = bytes / (1024 ** 2);
  return `${mb.toFixed(0)} MB`;
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusDot({ status, label }: { status: "ok" | "warn" | "error" | "idle"; label: string }) {
  const colors = {
    ok: "bg-accent-500",
    warn: "bg-yellow-500",
    error: "bg-danger-500",
    idle: "bg-surface-300",
  };
  return (
    <span className="inline-flex items-center gap-1.5 text-sm">
      <span className={`h-2 w-2 rounded-full ${colors[status]}`} />
      {label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Metric card
// ---------------------------------------------------------------------------

function MetricCard({
  label,
  value,
  sub,
  accent,
}: {
  label: string;
  value: string;
  sub?: string;
  accent?: boolean;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <p className="text-xs font-medium uppercase tracking-wide text-surface-800/50 mb-1">
        {label}
      </p>
      <p className={`text-2xl font-bold ${accent ? "text-brand-600" : "text-surface-900"}`}>
        {value}
      </p>
      {sub && <p className="text-xs text-surface-800/50 mt-1">{sub}</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main dashboard
// ---------------------------------------------------------------------------

export function ProviderDashboard() {
  const user = useAuthStore((s) => s.user);
  const [data, setData] = useState<ProviderDashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);

  const companyId = user?.publicKey ?? "";
  const hasWallet = user?.ergoAddress != null && user.ergoAddress.length > 0;

  const load = useCallback(async () => {
    if (!companyId) return;
    try {
      const result = await fetchProviderDashboardData(companyId);
      setData(result);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err : new Error("Failed to load dashboard data"));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    load();
  }, [load]);

  // Auto-refresh every 30s when enabled
  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(load, 30_000);
    return () => clearInterval(interval);
  }, [autoRefresh, load]);

  // ── Gate: not authenticated ──
  if (!user) {
    return (
      <div className="max-w-4xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Provider Dashboard</h1>
        <p className="text-surface-800/60 mb-8">
          Monitor your node health, PoNW score, and earnings.
        </p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">
            Sign in to access the provider dashboard
          </p>
          <Link
            href="/signin"
            className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Sign in
          </Link>
        </div>
      </div>
    );
  }

  // ── Gate: no wallet linked ──
  if (!hasWallet && !loading && data && !data.hasWallet) {
    return (
      <div className="max-w-4xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Provider Dashboard</h1>
        <p className="text-surface-800/60 mb-8">
          Monitor your node health, PoNW score, and earnings.
        </p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <div className="text-4xl mb-4 opacity-30">&#x26A1;</div>
          <h2 className="font-semibold mb-2">Link an Ergo Wallet</h2>
          <p className="text-sm text-surface-800/60 mb-6 max-w-md mx-auto">
            To participate as a compute provider and earn ERG, you need to link an
            Ergo wallet in Advanced Settings. This wallet will receive your settlements.
          </p>
          <Link
            href="/settings"
            className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Open Settings
          </Link>
        </div>
      </div>
    );
  }

  const nodeStatus = data?.nodeStatus;
  const peers = data?.peers ?? [];
  const aiPoints = data?.aiPoints;
  const providerScore = data?.providerScore;
  const hardware = data?.hardware;
  const settlements = data?.settlements ?? [];

  const synced = nodeStatus?.synced ?? false;
  const behind = nodeStatus ? nodeStatus.bestHeight - nodeStatus.height : 0;

  return (
    <div className="max-w-5xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold mb-1">Provider Dashboard</h1>
          <p className="text-surface-800/60">
            Real-time monitoring of your node, PoNW score, and earnings.
          </p>
        </div>
        <div className="flex items-center gap-3 shrink-0">
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
              autoRefresh
                ? "bg-accent-500/10 text-accent-600 border border-accent-500/30"
                : "bg-surface-100 text-surface-800/60 border border-surface-200"
            }`}
          >
            {autoRefresh ? "Auto-refresh ON" : "Auto-refresh OFF"}
          </button>
          <button
            onClick={load}
            className="px-3 py-1.5 rounded-lg text-xs font-medium bg-surface-100 text-surface-800/70 hover:bg-surface-200 transition-colors"
          >
            Refresh
          </button>
        </div>
      </div>

      {/* Error display */}
      {error && (
        <ApiErrorDisplay
          error={error}
          onRetry={load}
          className="mb-6"
        />
      )}

      {loading ? (
        <SkeletonMetricGrid />
      ) : (
        <ErrorBoundary context="Provider Metrics">
          <>
            {/* ── Realtime metrics with sparklines ── */}
            <RealtimeMetrics
              aiPoints={aiPoints ?? null}
              settlements={settlements}
              providerScore={providerScore ?? null}
              onRefresh={autoRefresh ? load : undefined}
            />

            {/* ── Key metrics row ── */}
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-8 animate-fade-in">
              <MetricCard
                label="PoNW Score"
                value={formatNumber(aiPoints?.aiPoints ?? 0)}
                sub="AI Points (30d)"
                accent
              />
              <MetricCard
                label="Requests Handled"
                value={formatNumber(aiPoints?.byModel.reduce((sum, m) => sum + m.inputTokens, 0) ?? 0)}
                sub={`${formatNumber(aiPoints?.totalInputTokens ?? 0)} in · ${formatNumber(aiPoints?.totalOutputTokens ?? 0)} out`}
              />
              <MetricCard
                label="ERG Earned"
                value={formatErg(settlements.reduce((s, tx) => s + tx.amountNanoerg, 0))}
                sub={`${settlements.filter((t) => t.status === "confirmed").length} confirmed settlements`}
              />
              <MetricCard
                label="Provider Score"
                value={`${(providerScore?.weightedCompositeScore ?? 0).toFixed(0)}/100`}
                sub={`Best: ${(providerScore?.bestCompositeScore ?? 0).toFixed(0)}`}
                accent
              />
            </div>
          </>
        </ErrorBoundary>
      )}

      {!loading && (
        <ErrorBoundary context="Provider Details">
          <div className="animate-fade-in">
            {/* ── Earnings chart (full width) ── */}
            <div className="mb-8">
              <EarningsChart settlements={settlements} />
            </div>

            {/* ── Two-column layout: Node Health + GPU Hardware ── */}
            <div className="grid gap-6 lg:grid-cols-2 mb-8">
              {/* Node Health */}
              <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
                <h2 className="font-semibold mb-4 flex items-center gap-2">
                  <span className="text-lg">&#x1F310;</span> Node Health
                </h2>

                {!nodeStatus ? (
                  <div className="text-sm text-surface-800/40 py-4">
                    xergon-agent not reachable. Is your node running?
                    <br />
                    <code className="text-xs bg-surface-100 px-1.5 py-0.5 rounded mt-1 inline-block">
                      Expected at http://127.0.0.1:9090
                    </code>
                  </div>
                ) : (
                  <div className="space-y-4">
                    <div className="flex items-center justify-between">
                      <StatusDot
                        status={synced ? (behind > 10 ? "warn" : "ok") : "error"}
                        label={synced ? (behind > 10 ? `Syncing (${behind} blocks behind)` : "Synced") : "Not synced"}
                      />
                      <span className="text-xs text-surface-800/40 font-mono">
                        v{nodeStatus.version}
                      </span>
                    </div>

                    <div className="grid grid-cols-2 gap-4 text-sm">
                      <div>
                        <span className="text-surface-800/50 block text-xs mb-0.5">Height</span>
                        <span className="font-mono font-medium">{formatNumber(nodeStatus.height)}</span>
                        {behind > 0 && (
                          <span className="text-xs text-surface-800/40 ml-1">
                            / {formatNumber(nodeStatus.bestHeight)}
                          </span>
                        )}
                      </div>
                      <div>
                        <span className="text-surface-800/50 block text-xs mb-0.5">Peers</span>
                        <span className="font-medium">{nodeStatus.peers}</span>
                      </div>
                      <div>
                        <span className="text-surface-800/50 block text-xs mb-0.5">Uptime</span>
                        <span className="font-medium">{formatUptime(nodeStatus.uptimeSeconds)}</span>
                      </div>
                      <div>
                        <span className="text-surface-800/50 block text-xs mb-0.5">Address</span>
                        <span className="font-mono text-xs">
                          {nodeStatus.ergoAddress
                            ? `${nodeStatus.ergoAddress.slice(0, 9)}...${nodeStatus.ergoAddress.slice(-4)}`
                            : "Not set"}
                        </span>
                      </div>
                    </div>

                    {/* Peer list */}
                    {peers.length > 0 && (
                      <div>
                        <h3 className="text-xs font-medium uppercase tracking-wide text-surface-800/50 mb-2">
                          Connected Peers ({peers.length})
                        </h3>
                        <div className="max-h-32 overflow-y-auto space-y-1">
                          {peers.slice(0, 10).map((p, i) => (
                            <div key={i} className="flex items-center justify-between text-xs font-mono text-surface-800/70 py-0.5">
                              <span className="truncate max-w-[200px]">{p.address}</span>
                              <span className="flex items-center gap-2">
                                <span className="text-surface-800/40">h:{formatNumber(p.height)}</span>
                                <span className={`px-1.5 py-0.5 rounded text-[10px] ${
                                  p.connectionType === "direct"
                                    ? "bg-brand-100 text-brand-700"
                                    : "bg-surface-100 text-surface-600"
                                }`}>
                                  {p.connectionType}
                                </span>
                              </span>
                            </div>
                          ))}
                          {peers.length > 10 && (
                            <p className="text-xs text-surface-800/40">
                              +{peers.length - 10} more peers
                            </p>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </section>

              {/* GPU Hardware */}
              <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
                <h2 className="font-semibold mb-4 flex items-center gap-2">
                  <span className="text-lg">&#x1F4BB;</span> GPU Hardware
                </h2>

                {!hardware || hardware.devices.length === 0 ? (
                  <div className="text-sm text-surface-800/40 py-4">
                    No GPU detected. Run hardware detection to register your device.
                  </div>
                ) : (
                  <div className="space-y-3">
                    {hardware.devices.map((dev, i) => (
                      <div
                        key={i}
                        className="flex items-center justify-between p-3 rounded-lg bg-surface-50 border border-surface-100"
                      >
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-sm">{dev.deviceName}</span>
                            {dev.isActive && (
                              <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-accent-500/10 text-accent-600">
                                Active
                              </span>
                            )}
                          </div>
                          <div className="text-xs text-surface-800/50 mt-0.5">
                            {dev.vendor} &middot; {formatBytes(Number(dev.vramBytes))} VRAM
                            {dev.computeVersion && ` &middot; Compute ${dev.computeVersion}`}
                          </div>
                        </div>
                        <span className="text-xs text-surface-800/40 capitalize">
                          {dev.detectionMethod.replace(/_/g, " ")}
                        </span>
                      </div>
                    ))}
                    {hardware.lastReportedAt && (
                      <p className="text-xs text-surface-800/40">
                        Last reported: {timeAgo(hardware.lastReportedAt)}
                      </p>
                    )}
                  </div>
                )}
              </section>
            </div>

            {/* ── Model Performance chart (replaces plain PoNW table) ── */}
            <div className="mb-8">
              <ModelPerformanceChart aiPoints={aiPoints ?? null} />
            </div>

            {/* ── Two-column layout: Settlement History + Request Heatmap ── */}
            <div className="grid gap-6 lg:grid-cols-2 mb-8">
              {/* Settlement History */}
              <section className="rounded-xl border border-surface-200 bg-surface-0 p-6">
                <h2 className="font-semibold mb-4 flex items-center gap-2">
                  <span className="text-lg">&#x1F4B0;</span> Settlement History
                </h2>

                {settlements.length === 0 ? (
                  <p className="text-sm text-surface-800/40 py-4">
                    No settlements yet. ERG earnings are sent to your linked wallet.
                  </p>
                ) : (
                  <div className="space-y-2">
                    {settlements.map((tx) => (
                      <div
                        key={tx.id}
                        className="flex items-center justify-between p-3 rounded-lg bg-surface-50 border border-surface-100 text-sm"
                      >
                        <div>
                          <div className="flex items-center gap-2">
                            <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${
                              tx.status === "confirmed"
                                ? "bg-accent-500/10 text-accent-600"
                                : tx.status === "pending"
                                ? "bg-yellow-500/10 text-yellow-600"
                                : "bg-danger-500/10 text-danger-600"
                            }`}>
                              {tx.status}
                            </span>
                            <span className="font-mono text-xs text-surface-800/50">
                              {tx.txId}
                            </span>
                          </div>
                          <span className="text-xs text-surface-800/40 mt-0.5 block">
                            {timeAgo(tx.createdAt)}
                          </span>
                        </div>
                        <div className="text-right">
                          <span className="font-medium">{formatErg(tx.amountNanoerg)}</span>
                          <div className="text-xs text-surface-800/40">
                            {formatErg(tx.nanoergConverted ?? 0)} ERG
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              {/* Request Heatmap */}
              <RequestHeatmap aiPoints={aiPoints ?? null} />
            </div>

            {/* ── Model Pricing ── */}
            <PricingSection />
          </div>
        </ErrorBoundary>
      )}
    </div>
  );
}
