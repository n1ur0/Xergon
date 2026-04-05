"use client";

import { type OnChainTransaction, formatNanoerg, truncateTxId, explorerUrl, timeAgo } from "@/lib/api/transactions";

// ---------------------------------------------------------------------------
// Type badge
// ---------------------------------------------------------------------------

const TYPE_STYLES: Record<OnChainTransaction["type"], string> = {
  staking: "bg-blue-500/10 text-blue-600 border-blue-500/20",
  settlement: "bg-emerald-500/10 text-emerald-600 border-emerald-500/20",
  inference_payment: "bg-purple-500/10 text-purple-600 border-purple-500/20",
  reward: "bg-amber-500/10 text-amber-600 border-amber-500/20",
};

const TYPE_LABELS: Record<OnChainTransaction["type"], string> = {
  staking: "Staking",
  settlement: "Settlement",
  inference_payment: "Payment",
  reward: "Reward",
};

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

const STATUS_STYLES: Record<OnChainTransaction["status"], string> = {
  confirmed: "bg-emerald-500/10 text-emerald-600",
  pending: "bg-yellow-500/10 text-yellow-600",
  failed: "bg-danger-500/10 text-danger-600",
};

// ---------------------------------------------------------------------------
// Sort config
// ---------------------------------------------------------------------------

export type SortField = "date" | "type" | "amount" | "status" | "confirmations";
export type SortDir = "asc" | "desc";

export interface TxTableProps {
  transactions: OnChainTransaction[];
  sortField: SortField;
  sortDir: SortDir;
  onSort: (field: SortField) => void;
  isLoading?: boolean;
}

function SortableHeader({
  label,
  field,
  currentField,
  currentDir,
  onSort,
}: {
  label: string;
  field: SortField;
  currentField: SortField;
  currentDir: SortDir;
  onSort: (field: SortField) => void;
}) {
  const isActive = currentField === field;
  const sortValue = isActive ? (currentDir === "asc" ? "ascending" : "descending") : "none";
  return (
    <button
      type="button"
      onClick={() => onSort(field)}
      aria-sort={sortValue}
      className="flex items-center gap-1 text-xs font-medium uppercase tracking-wide text-surface-800/60 hover:text-surface-900 transition-colors"
    >
      {label}
      <span className={`inline-flex flex-col leading-none ${isActive ? "text-surface-900" : "text-surface-800/30"}`} aria-hidden="true">
        <span className={`text-[8px] ${isActive && currentDir === "asc" ? "text-surface-900" : ""}`}>&#9650;</span>
        <span className={`text-[8px] -mt-0.5 ${isActive && currentDir === "desc" ? "text-surface-900" : ""}`}>&#9660;</span>
      </span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// Skeleton rows
// ---------------------------------------------------------------------------

function TableSkeleton() {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-surface-200">
            {Array.from({ length: 7 }).map((_, i) => (
              <th key={i} className="px-3 py-3 text-left">
                <div className="h-3 w-16 bg-surface-200 rounded animate-pulse" />
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {Array.from({ length: 8 }).map((_, i) => (
            <tr key={i} className="border-b border-surface-100">
              {Array.from({ length: 7 }).map((_, j) => (
                <td key={j} className="px-3 py-3">
                  <div className="h-4 bg-surface-100 rounded animate-pulse" style={{ width: `${40 + Math.random() * 60}%` }} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---------------------------------------------------------------------------
// TxTable component
// ---------------------------------------------------------------------------

export function TxTable({ transactions, sortField, sortDir, onSort, isLoading }: TxTableProps) {
  if (isLoading) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden" aria-busy="true" aria-label="Loading transactions">
        <TableSkeleton />
      </div>
    );
  }

  if (transactions.length === 0) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
        <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="mx-auto text-surface-800/20 mb-4">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <line x1="16" y1="13" x2="8" y2="13" />
          <line x1="16" y1="17" x2="8" y2="17" />
        </svg>
        <p className="text-surface-800/50 font-medium mb-1">No transactions yet</p>
        <p className="text-xs text-surface-800/30">
          Transactions will appear here once you interact with the marketplace.
        </p>
      </div>
    );
  }

  // Determine if amount is earned (positive) or spent (negative)
  const earnedTypes = new Set<OnChainTransaction["type"]>(["settlement", "reward"]);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-surface-200 bg-surface-50/50">
              <th scope="col" className="px-3 py-3 text-left">
                <span className="text-xs font-medium uppercase tracking-wide text-surface-800/60">Tx ID</span>
              </th>
              <th scope="col" className="px-3 py-3 text-left">
                <SortableHeader label="Type" field="type" currentField={sortField} currentDir={sortDir} onSort={onSort} />
              </th>
              <th scope="col" className="px-3 py-3 text-left">
                <SortableHeader label="Amount" field="amount" currentField={sortField} currentDir={sortDir} onSort={onSort} />
              </th>
              <th scope="col" className="px-3 py-3 text-left">
                <span className="text-xs font-medium uppercase tracking-wide text-surface-800/60">Model</span>
              </th>
              <th scope="col" className="px-3 py-3 text-left hidden md:table-cell">
                <span className="text-xs font-medium uppercase tracking-wide text-surface-800/60">Counterpart</span>
              </th>
              <th scope="col" className="px-3 py-3 text-left">
                <SortableHeader label="Status" field="status" currentField={sortField} currentDir={sortDir} onSort={onSort} />
              </th>
              <th scope="col" className="px-3 py-3 text-left hidden lg:table-cell">
                <SortableHeader label="Confirmations" field="confirmations" currentField={sortField} currentDir={sortDir} onSort={onSort} />
              </th>
              <th scope="col" className="px-3 py-3 text-left">
                <SortableHeader label="Date" field="date" currentField={sortField} currentDir={sortDir} onSort={onSort} />
              </th>
            </tr>
          </thead>
          <tbody>
            {transactions.map((tx) => {
              const isEarned = earnedTypes.has(tx.type);
              return (
                <tr
                  key={tx.id}
                  className="border-b border-surface-100 hover:bg-surface-50/70 transition-colors"
                >
                  {/* Tx ID */}
                  <td className="px-3 py-3">
                    <a
                      href={explorerUrl(tx.txId)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="font-mono text-xs text-brand-600 hover:text-brand-700 hover:underline transition-colors"
                    >
                      {truncateTxId(tx.txId)}
                    </a>
                  </td>

                  {/* Type */}
                  <td className="px-3 py-3">
                    <span className={`inline-flex px-2 py-0.5 rounded-md text-[11px] font-medium border ${TYPE_STYLES[tx.type]}`}>
                      {TYPE_LABELS[tx.type]}
                    </span>
                  </td>

                  {/* Amount */}
                  <td className="px-3 py-3">
                    <span className={`font-medium font-mono text-sm ${isEarned ? "text-emerald-600" : "text-danger-500"}`}>
                      {isEarned ? "+" : "-"}{formatNanoerg(tx.amountNanoerg)}
                    </span>
                  </td>

                  {/* Model */}
                  <td className="px-3 py-3">
                    {tx.model ? (
                      <span className="text-xs text-surface-800/70">{tx.model}</span>
                    ) : (
                      <span className="text-xs text-surface-800/20">&mdash;</span>
                    )}
                  </td>

                  {/* Counterpart */}
                  <td className="px-3 py-3 hidden md:table-cell">
                    {tx.counterpart ? (
                      <span className="font-mono text-xs text-surface-800/50">
                        {tx.counterpart.length > 16
                          ? `${tx.counterpart.slice(0, 10)}...${tx.counterpart.slice(-4)}`
                          : tx.counterpart}
                      </span>
                    ) : (
                      <span className="text-xs text-surface-800/20">&mdash;</span>
                    )}
                  </td>

                  {/* Status */}
                  <td className="px-3 py-3">
                    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium ${STATUS_STYLES[tx.status]}`}>
                      <span className={`h-1.5 w-1.5 rounded-full ${
                        tx.status === "confirmed" ? "bg-emerald-500" :
                        tx.status === "pending" ? "bg-yellow-500 animate-pulse" :
                        "bg-danger-500"
                      }`} aria-hidden="true" />
                      {tx.status}
                    </span>
                  </td>

                  {/* Confirmations */}
                  <td className="px-3 py-3 hidden lg:table-cell">
                    <span className="text-xs text-surface-800/60 font-mono">
                      {tx.status === "confirmed"
                        ? `${tx.confirmations} conf`
                        : tx.status === "pending"
                          ? "..."
                          : "N/A"}
                    </span>
                  </td>

                  {/* Date */}
                  <td className="px-3 py-3">
                    <div className="text-xs text-surface-800/60">{timeAgo(tx.timestamp)}</div>
                    <div className="text-[10px] text-surface-800/30 font-mono mt-0.5">
                      #{tx.blockHeight.toLocaleString()}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
