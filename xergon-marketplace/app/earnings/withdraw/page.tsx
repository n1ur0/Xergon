"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { WithdrawalForm } from "@/components/earnings/WithdrawalForm";
import { StatusBadge } from "@/components/earnings/StatusBadge";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WithdrawalRecord {
  id: string;
  amountNanoErg: number;
  destinationAddress: string;
  txId: string;
  status: "pending" | "completed" | "failed";
  createdAt: string;
  completedAt?: string;
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function formatErg(nanoErg: number): string {
  return (nanoErg / 1e9).toFixed(4);
}

function truncateAddr(addr: string): string {
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function WithdrawPage() {
  const [balanceNanoErg, setBalanceNanoErg] = useState(0);
  const [history, setHistory] = useState<WithdrawalRecord[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [successTxId, setSuccessTxId] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const [earningsRes, historyRes] = await Promise.all([
        fetch("/api/earnings"),
        fetch("/api/earnings/history"),
      ]);

      if (!earningsRes.ok || !historyRes.ok) {
        throw new Error("Failed to load withdrawal data");
      }

      const earningsData = await earningsRes.json();
      const historyData = await historyRes.json();

      setBalanceNanoErg(earningsData.summary.totalEarningsNanoErg);
      setHistory(historyData);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load data",
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  function handleWithdrawSuccess(txId: string) {
    setSuccessTxId(txId);
    // Refresh data to get updated balance
    loadData();
  }

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center gap-2 text-sm text-surface-800/40 mb-2">
          <Link
            href="/earnings"
            className="hover:text-brand-600 transition-colors"
          >
            Earnings
          </Link>
          <span>/</span>
          <span className="text-surface-800/60">Withdraw</span>
        </div>
        <h1 className="text-2xl font-bold text-surface-900">
          Withdraw Funds
        </h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          Withdraw your earned ERG to any Ergo wallet address
        </p>
      </div>

      {/* Error state */}
      {error && !isLoading && (
        <div className="mb-6 rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Success message */}
      {successTxId && (
        <div className="mb-6 rounded-lg border border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 px-4 py-3">
          <div className="flex items-start gap-3">
            <svg
              className="h-5 w-5 text-emerald-500 mt-0.5 flex-shrink-0"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={2}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <div>
              <p className="text-sm font-medium text-emerald-800 dark:text-emerald-300">
                Withdrawal submitted successfully!
              </p>
              <p className="text-xs text-emerald-700/70 dark:text-emerald-400/70 mt-1">
                Transaction ID:{" "}
                <a
                  href={`https://explorer.ergoplatform.com/en/transactions/${successTxId}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="font-mono underline hover:text-emerald-600 dark:hover:text-emerald-300"
                >
                  {successTxId.slice(0, 16)}...{successTxId.slice(-8)}
                </a>
              </p>
              <button
                type="button"
                onClick={() => setSuccessTxId(null)}
                className="text-xs text-emerald-700/70 dark:text-emerald-400/70 mt-1 underline hover:text-emerald-600"
              >
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-5 gap-6">
        {/* Withdrawal form */}
        <div className="lg:col-span-3">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 space-y-5">
              <SkeletonPulse className="h-12 w-full" />
              <SkeletonPulse className="h-10 w-full" />
              <SkeletonPulse className="h-10 w-full" />
              <SkeletonPulse className="h-10 w-full" />
              <SkeletonPulse className="h-10 w-full" />
            </div>
          ) : (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 shadow-sm">
              <WithdrawalForm
                balanceNanoErg={balanceNanoErg}
                onSuccess={handleWithdrawSuccess}
              />
            </div>
          )}
        </div>

        {/* Withdrawal history sidebar */}
        <div className="lg:col-span-2">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-5 w-36 mb-4" />
              <div className="space-y-4">
                {Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="space-y-2 pb-3 border-b border-surface-100 last:border-0">
                    <SkeletonPulse className="h-4 w-20" />
                    <SkeletonPulse className="h-3 w-32" />
                    <SkeletonPulse className="h-3 w-28" />
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <ErrorBoundary context="Withdrawal History">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
                <h2 className="text-base font-semibold text-surface-900 mb-4">
                  Withdrawal History
                </h2>

                {history.length === 0 ? (
                  <div className="text-sm text-surface-800/40 py-8 text-center">
                    No withdrawal history
                  </div>
                ) : (
                  <div className="space-y-0 max-h-[500px] overflow-y-auto">
                    {history.map((w) => (
                      <div
                        key={w.id}
                        className="flex items-start justify-between gap-3 py-3 border-b border-surface-100 last:border-0"
                      >
                        <div className="min-w-0">
                          <div className="text-sm font-medium text-surface-900">
                            {formatErg(w.amountNanoErg)} ERG
                          </div>
                          <div className="text-xs text-surface-800/40 mt-0.5 truncate font-mono">
                            {truncateAddr(w.destinationAddress)}
                          </div>
                          <div className="text-xs text-surface-800/30 mt-0.5">
                            {formatDate(w.createdAt)}
                          </div>
                          {w.txId && (
                            <a
                              href={`https://explorer.ergoplatform.com/en/transactions/${w.txId}`}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="text-xs text-brand-600 hover:text-brand-700 font-mono mt-0.5 inline-block truncate max-w-[180px]"
                            >
                              View tx
                            </a>
                          )}
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

      {/* Info banner */}
      <div className="mt-6 rounded-lg border border-surface-200 bg-surface-50 dark:bg-surface-900 px-4 py-3 text-xs text-surface-800/50">
        <strong>Note:</strong> Withdrawals are processed on the Ergo blockchain.
        Network confirmation typically takes 2-10 minutes. Minimum withdrawal
        amount is 0.001 ERG with a network fee of ~0.001 ERG.
      </div>
    </div>
  );
}
