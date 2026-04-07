"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { SummaryCard } from "@/components/earnings/SummaryCard";
import { EarningsChart } from "@/components/earnings/EarningsChart";
import { StatusBadge } from "@/components/earnings/StatusBadge";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EarningsDaily {
  date: string;
  earningsNanoErg: number;
  requests: number;
  tokensServed: number;
  uniqueUsers: number;
}

interface EarningsByModel {
  modelId: string;
  earningsNanoErg: number;
  requests: number;
  tokensServed: number;
}

interface WithdrawalRecord {
  id: string;
  amountNanoErg: number;
  destinationAddress: string;
  txId: string;
  status: "pending" | "completed" | "failed";
  createdAt: string;
  completedAt?: string;
}

interface EarningsData {
  provider: { address: string; region: string; models: string[] };
  summary: {
    totalEarningsNanoErg: number;
    totalRequests: number;
    totalTokensServed: number;
    averageLatencyMs: number;
    uptime: number;
    period: { start: string; end: string };
  };
  daily: EarningsDaily[];
  byModel: EarningsByModel[];
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function formatErg(nanoErg: number): string {
  return (nanoErg / 1e9).toFixed(4);
}

function formatNumber(n: number): string {
  return n.toLocaleString();
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(1) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return n.toString();
}

function truncateAddr(addr: string): string {
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ---------------------------------------------------------------------------
// Skeleton loaders
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function StatsSkeleton() {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      {Array.from({ length: 4 }).map((_, i) => (
        <div
          key={i}
          className="rounded-xl border border-surface-200 bg-surface-0 p-5"
        >
          <div className="flex items-center justify-between mb-3">
            <SkeletonPulse className="h-4 w-20" />
            <SkeletonPulse className="h-8 w-8 rounded-lg" />
          </div>
          <SkeletonPulse className="h-7 w-28 mb-1.5" />
          <SkeletonPulse className="h-3 w-16" />
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function EarningsPage() {
  const [earnings, setEarnings] = useState<EarningsData | null>(null);
  const [withdrawals, setWithdrawals] = useState<WithdrawalRecord[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const [earningsRes, historyRes] = await Promise.all([
        fetch("/api/earnings"),
        fetch("/api/earnings/history"),
      ]);

      if (!earningsRes.ok || !historyRes.ok) {
        throw new Error("Failed to load earnings data");
      }

      const earningsData = await earningsRes.json();
      const historyData = await historyRes.json();

      setEarnings(earningsData);
      setWithdrawals(historyData.slice(0, 5));
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load earnings data",
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Auto-refresh every 60s
  useEffect(() => {
    const interval = setInterval(loadData, 60_000);
    return () => clearInterval(interval);
  }, [loadData]);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">
            Provider Earnings
          </h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Track your inference revenue and manage withdrawals
          </p>
        </div>
        <Link
          href="/earnings/withdraw"
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-brand-700 self-start"
        >
          <svg
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={2}
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M2.25 18.75a60.07 60.07 0 0115.797 2.101c.727.198 1.453-.342 1.453-1.096V18.75M3.75 4.5v.75A.75.75 0 013 6h-.75m0 0v-.375c0-.621.504-1.125 1.125-1.125H20.25M2.25 6v9m18-10.5v.75c0 .414.336.75.75.75h.75m-1.5-1.5h.375c.621 0 1.125.504 1.125 1.125v9.75c0 .621-.504 1.125-1.125 1.125h-.375m1.5-1.5H21a.75.75 0 00-.75.75v.75m0 0H3.75m0 0h-.375a1.125 1.125 0 01-1.125-1.125V15m1.5 1.5v-.75A.75.75 0 003 15h-.75M15 10.5a3 3 0 11-6 0 3 3 0 016 0zm3 0h.008v.008H18V10.5zm-12 0h.008v.008H6V10.5z"
            />
          </svg>
          Withdraw Funds
        </Link>
      </div>

      {/* Error state */}
      {error && !isLoading && (
        <div className="mb-6 rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Summary cards */}
      <div className="mb-6">
        {isLoading ? (
          <StatsSkeleton />
        ) : earnings ? (
          <ErrorBoundary context="Earnings Summary">
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              <SummaryCard
                title="Total Earnings"
                value={`${formatErg(earnings.summary.totalEarningsNanoErg)} ERG`}
                subtitle={`${earnings.summary.period.start} - ${earnings.summary.period.end}`}
                trend="up"
                trendValue="+12.3%"
                icon={
                  <svg
                    className="h-5 w-5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M12 6v12m-3-2.818l.879.659c1.171.879 3.07.879 4.242 0 1.172-.879 1.172-2.303 0-3.182C13.536 12.219 12.768 12 12 12c-.725 0-1.45-.22-2.003-.659-1.106-.879-1.106-2.303 0-3.182s2.9-.879 4.006 0l.415.33M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                    />
                  </svg>
                }
              />
              <SummaryCard
                title="Total Requests"
                value={formatNumber(earnings.summary.totalRequests)}
                subtitle="Last 30 days"
                trend="up"
                trendValue="+8.7%"
                icon={
                  <svg
                    className="h-5 w-5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"
                    />
                  </svg>
                }
              />
              <SummaryCard
                title="Tokens Served"
                value={formatTokens(earnings.summary.totalTokensServed)}
                subtitle="Across all models"
                trend="up"
                trendValue="+15.2%"
                icon={
                  <svg
                    className="h-5 w-5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M17.25 6.75L22.5 12l-5.25 5.25m-10.5 0L1.5 12l5.25-5.25m7.5-3l-4.5 16.5"
                    />
                  </svg>
                }
              />
              <SummaryCard
                title="Avg Latency"
                value={`${earnings.summary.averageLatencyMs}ms`}
                subtitle={`Uptime: ${earnings.summary.uptime}%`}
                trend="neutral"
                trendValue="Stable"
                icon={
                  <svg
                    className="h-5 w-5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
                    />
                  </svg>
                }
              />
            </div>
          </ErrorBoundary>
        ) : null}
      </div>

      {/* Chart + Recent Withdrawals */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        {/* Earnings chart */}
        <div className="lg:col-span-2">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <div className="flex items-center justify-between mb-4">
                <SkeletonPulse className="h-5 w-36" />
                <SkeletonPulse className="h-7 w-24 rounded-lg" />
              </div>
              <SkeletonPulse className="h-[260px] w-full" />
            </div>
          ) : earnings ? (
            <ErrorBoundary context="Earnings Chart">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-base font-semibold text-surface-900">
                    Earnings Over Time
                  </h2>
                  <span className="rounded-lg bg-surface-100 px-3 py-1 text-xs font-medium text-surface-800/50">
                    Last 30 days
                  </span>
                </div>
                <EarningsChart
                  data={earnings.daily.map((d) => ({
                    date: d.date,
                    value: d.earningsNanoErg,
                  }))}
                />
              </div>
            </ErrorBoundary>
          ) : null}
        </div>

        {/* Recent withdrawals */}
        <div>
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-5 w-36 mb-4" />
              <div className="space-y-3">
                {Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="space-y-2">
                    <SkeletonPulse className="h-4 w-20" />
                    <SkeletonPulse className="h-3 w-28" />
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <ErrorBoundary context="Recent Withdrawals">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-base font-semibold text-surface-900">
                    Recent Withdrawals
                  </h2>
                  <Link
                    href="/earnings/withdraw"
                    className="text-xs text-brand-600 hover:text-brand-700 font-medium"
                  >
                    Withdraw
                  </Link>
                </div>

                {withdrawals.length === 0 ? (
                  <div className="text-sm text-surface-800/40 py-8 text-center">
                    No withdrawals yet
                  </div>
                ) : (
                  <div className="space-y-4">
                    {withdrawals.map((w) => (
                      <div
                        key={w.id}
                        className="flex items-start justify-between gap-3 pb-3 border-b border-surface-100 last:border-0 last:pb-0"
                      >
                        <div className="min-w-0">
                          <div className="text-sm font-medium text-surface-900">
                            {formatErg(w.amountNanoErg)} ERG
                          </div>
                          <div className="text-xs text-surface-800/40 mt-0.5 truncate">
                            {truncateAddr(w.destinationAddress)}
                          </div>
                          <div className="text-xs text-surface-800/30 mt-0.5">
                            {formatDate(w.createdAt)}
                          </div>
                        </div>
                        <StatusBadge status={w.status} />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </ErrorBoundary>
          )}
        </div>
      </div>

      {/* By Model breakdown */}
      {isLoading ? (
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
          <div className="px-5 py-4 border-b border-surface-100">
            <SkeletonPulse className="h-5 w-32 mb-1" />
            <SkeletonPulse className="h-3 w-48" />
          </div>
          <div className="space-y-0">
            {Array.from({ length: 5 }).map((_, i) => (
              <div
                key={i}
                className="flex items-center gap-4 px-5 py-3 border-b border-surface-50"
              >
                <SkeletonPulse className="h-4 w-32" />
                <div className="flex-1" />
                <SkeletonPulse className="h-4 w-20" />
                <SkeletonPulse className="h-4 w-16" />
                <SkeletonPulse className="h-4 w-16" />
                <SkeletonPulse className="h-1.5 w-16 rounded-full" />
              </div>
            ))}
          </div>
        </div>
      ) : earnings ? (
        <ErrorBoundary context="Model Breakdown">
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden shadow-sm">
            <div className="px-5 py-4 border-b border-surface-100">
              <h2 className="text-base font-semibold text-surface-900">
                Earnings by Model
              </h2>
              <p className="text-xs text-surface-800/40 mt-0.5">
                Revenue breakdown across your hosted models
              </p>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-surface-100">
                    <th className="text-left px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">
                      Model
                    </th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">
                      Earnings
                    </th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider hidden sm:table-cell">
                      Requests
                    </th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider hidden md:table-cell">
                      Tokens
                    </th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">
                      Share
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {earnings.byModel.map((m) => {
                    const totalEarnings = earnings.byModel.reduce(
                      (s, b) => s + b.earningsNanoErg,
                      0,
                    );
                    const share =
                      totalEarnings > 0
                        ? (m.earningsNanoErg / totalEarnings) * 100
                        : 0;

                    return (
                      <tr
                        key={m.modelId}
                        className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors"
                      >
                        <td className="px-5 py-3 font-medium text-surface-900 font-mono text-xs">
                          {m.modelId}
                        </td>
                        <td className="px-5 py-3 text-right text-surface-900">
                          {formatErg(m.earningsNanoErg)} ERG
                        </td>
                        <td className="px-5 py-3 text-right text-surface-800/70 hidden sm:table-cell">
                          {formatNumber(m.requests)}
                        </td>
                        <td className="px-5 py-3 text-right text-surface-800/70 hidden md:table-cell">
                          {formatTokens(m.tokensServed)}
                        </td>
                        <td className="px-5 py-3 text-right">
                          <div className="inline-flex items-center gap-2">
                            <div className="w-16 h-1.5 rounded-full bg-surface-100 overflow-hidden">
                              <div
                                className="h-full rounded-full bg-brand-500"
                                style={{ width: `${share}%` }}
                              />
                            </div>
                            <span className="text-xs text-surface-800/50 w-10 text-right">
                              {share.toFixed(1)}%
                            </span>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </ErrorBoundary>
      ) : null}

      {/* Footer */}
      {!isLoading && earnings && (
        <div className="text-xs text-surface-800/30 text-center mt-6">
          Data refreshes every 60 seconds. Showing estimated data.
        </div>
      )}
    </div>
  );
}
