"use client";

import { type TxSummary } from "@/lib/api/transactions";
import { formatNanoerg } from "@/lib/api/transactions";

interface TxSummaryCardsProps {
  summary: TxSummary;
  isLoading?: boolean;
}

function CardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 animate-pulse">
      <div className="h-3 w-20 bg-surface-200 rounded mb-2" />
      <div className="h-7 w-28 bg-surface-200 rounded mb-1" />
      <div className="h-3 w-16 bg-surface-100 rounded" />
    </div>
  );
}

export function TxSummaryCards({ summary, isLoading }: TxSummaryCardsProps) {
  if (isLoading) {
    return (
      <div className="grid gap-4 grid-cols-2 lg:grid-cols-4 mb-6">
        <CardSkeleton />
        <CardSkeleton />
        <CardSkeleton />
        <CardSkeleton />
      </div>
    );
  }

  const cards = [
    {
      label: "Total Spent",
      value: formatNanoerg(summary.totalSpent),
      sub: `${(summary.totalSpent / 1e9).toFixed(2)} ERG`,
      accent: "text-danger-500",
      icon: (
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
        </svg>
      ),
      bgAccent: "bg-danger-500/10",
    },
    {
      label: "Total Earned",
      value: formatNanoerg(summary.totalEarned),
      sub: `${(summary.totalEarned / 1e9).toFixed(2)} ERG`,
      accent: "text-accent-600",
      icon: (
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="12" y1="1" x2="12" y2="23" /><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
        </svg>
      ),
      bgAccent: "bg-accent-500/10",
    },
    {
      label: "Transactions",
      value: String(summary.totalTransactions),
      sub: summary.pendingCount > 0
        ? `${summary.pendingCount} pending`
        : "All confirmed",
      accent: "text-surface-900",
      icon: (
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <line x1="16" y1="13" x2="8" y2="13" />
          <line x1="16" y1="17" x2="8" y2="17" />
        </svg>
      ),
      bgAccent: "bg-surface-100",
    },
    {
      label: "Pending",
      value: String(summary.pendingCount),
      sub: summary.pendingCount === 0
        ? "No pending transactions"
        : "Awaiting confirmation",
      accent: summary.pendingCount > 0 ? "text-yellow-600" : "text-surface-800/50",
      icon: (
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      ),
      bgAccent: summary.pendingCount > 0 ? "bg-yellow-500/10" : "bg-surface-100",
    },
  ];

  return (
    <div className="grid gap-4 grid-cols-2 lg:grid-cols-4 mb-6">
      {cards.map((card) => (
        <div
          key={card.label}
          className="rounded-xl border border-surface-200 bg-surface-0 p-4"
        >
          <div className="flex items-center gap-2 mb-1">
            <div className={`${card.bgAccent} rounded-lg p-1.5 ${card.accent}`}>
              {card.icon}
            </div>
            <p className="text-xs font-medium uppercase tracking-wide text-surface-800/50">
              {card.label}
            </p>
          </div>
          <p className={`text-2xl font-bold ${card.accent}`}>
            {card.value}
          </p>
          {card.sub && (
            <p className="text-xs text-surface-800/50 mt-1">{card.sub}</p>
          )}
        </div>
      ))}
    </div>
  );
}
