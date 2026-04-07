"use client";

import { useState } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface BalanceDisplayProps {
  balanceNanoerg: number;
  onTopUp?: () => void;
  className?: string;
}

interface TransactionSummary {
  id: string;
  amountNanoerg: number;
  date: string;
  type: "credit" | "debit";
  description: string;
}

// ── Helpers ──

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
}

const ERG_USD_RATE = 1.85;

// Mock recent transactions
const MOCK_RECENT: TransactionSummary[] = [
  { id: "1", amountNanoerg: 500_000_000, date: "2 hours ago", type: "credit", description: "Top-up" },
  { id: "2", amountNanoerg: -50_000_000, date: "5 hours ago", type: "debit", description: "Model inference" },
  { id: "3", amountNanoerg: -20_000_000, date: "1 day ago", type: "debit", description: "Model inference" },
];

// ── Component ──

export function BalanceDisplay({ balanceNanoerg, onTopUp, className }: BalanceDisplayProps) {
  const [showDetails, setShowDetails] = useState(false);
  const ergBalance = nanoergToErg(balanceNanoerg);
  const usdBalance = (balanceNanoerg / 1e9 * ERG_USD_RATE).toFixed(2);

  return (
    <div className={cn("rounded-xl border border-surface-200 bg-surface-0 overflow-hidden", className)}>
      {/* Main balance display */}
      <div className="p-4">
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs text-surface-800/40 font-medium uppercase tracking-wide">
            ERG Balance
          </span>
          {onTopUp && (
            <button
              onClick={onTopUp}
              className="rounded-md bg-brand-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-brand-700 transition-colors"
            >
              + Top Up
            </button>
          )}
        </div>

        <div className="flex items-baseline gap-2">
          <span className="text-2xl font-bold text-surface-900">{ergBalance}</span>
          <span className="text-sm text-surface-800/40">ERG</span>
        </div>

        <div className="text-xs text-surface-800/30 mt-0.5">
          ~${usdBalance} USD
        </div>

        {/* Toggle recent transactions */}
        <button
          onClick={() => setShowDetails(!showDetails)}
          className="mt-3 text-xs text-surface-800/40 hover:text-surface-800/60 transition-colors flex items-center gap-1"
        >
          Recent activity
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className={cn("transition-transform", showDetails && "rotate-180")}
          >
            <path d="m6 9 6 6 6-6" />
          </svg>
        </button>
      </div>

      {/* Recent transactions */}
      {showDetails && (
        <div className="border-t border-surface-100 divide-y divide-surface-100">
          {MOCK_RECENT.map((tx) => (
            <div key={tx.id} className="flex items-center justify-between px-4 py-2.5">
              <div>
                <div className="text-xs text-surface-800/70">{tx.description}</div>
                <div className="text-[10px] text-surface-800/30">{tx.date}</div>
              </div>
              <span
                className={cn(
                  "text-xs font-medium",
                  tx.type === "credit" ? "text-green-600" : "text-surface-800/60",
                )}
              >
                {tx.type === "credit" ? "+" : ""}{nanoergToErg(Math.abs(tx.amountNanoerg))} ERG
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
