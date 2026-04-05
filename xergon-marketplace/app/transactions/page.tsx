"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuthStore } from "@/lib/stores/auth";
import {
  fetchUserTransactions,
  type OnChainTransaction,
  type TxSummary,
  type TransactionsResponse,
} from "@/lib/api/transactions";
import { TxSummaryCards } from "@/components/transactions/TxSummaryCards";
import { TxTable, type SortField, type SortDir } from "@/components/transactions/TxTable";
import { TxFilters, type TxFiltersState } from "@/components/transactions/TxFilters";
import { TxExport } from "@/components/transactions/TxExport";
import { TransactionsSkeleton } from "@/components/transactions/TransactionsSkeleton";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const PAGE_SIZE = 20;
const AUTO_REFRESH_MS = 60_000;

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

export default function TransactionsPage() {
  const router = useRouter();
  const user = useAuthStore((s) => s.user);

  const [transactions, setTransactions] = useState<OnChainTransaction[]>([]);
  const [summary, setSummary] = useState<TxSummary | null>(null);
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);

  const [filters, setFilters] = useState<TxFiltersState>({
    type: "all",
    status: "all",
    sort: "newest",
  });

  const [tableSort, setTableSort] = useState<{ field: SortField; dir: SortDir }>({
    field: "date",
    dir: "desc",
  });

  const address = user?.ergoAddress ?? "";

  // ── Fetch data ──
  const load = useCallback(async () => {
    if (!address) return;
    setLoading(true);
    setError(null);
    try {
      const res: TransactionsResponse = await fetchUserTransactions(address, page, PAGE_SIZE);
      setTransactions(res.transactions);
      setSummary(res.summary);
      setTotalPages(res.totalPages);
    } catch (err) {
      setError(err instanceof Error ? err : new Error("Failed to load transactions"));
    } finally {
      setLoading(false);
    }
  }, [address, page]);

  useEffect(() => {
    load();
  }, [load]);

  // Auto-refresh
  useEffect(() => {
    if (!autoRefresh || !address) return;
    const interval = setInterval(load, AUTO_REFRESH_MS);
    return () => clearInterval(interval);
  }, [autoRefresh, load, address]);

  // Reset to page 1 when filters change
  useEffect(() => {
    setPage(1);
  }, [filters.type, filters.status, filters.sort]);

  // ── Filter + sort transactions client-side ──
  const filteredTransactions = useMemo(() => {
    let items = [...transactions];

    // Type filter
    if (filters.type !== "all") {
      items = items.filter((tx) => tx.type === filters.type);
    }

    // Status filter
    if (filters.status !== "all") {
      items = items.filter((tx) => tx.status === filters.status);
    }

    // Client-side sort (table column sort overrides)
    const dir = tableSort.dir === "asc" ? 1 : -1;
    items.sort((a, b) => {
      switch (tableSort.field) {
        case "date":
          return (new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()) * (dir === -1 ? 1 : -1);
        case "type":
          return a.type.localeCompare(b.type) * dir;
        case "amount":
          return (a.amountNanoerg - b.amountNanoerg) * dir;
        case "status":
          return a.status.localeCompare(b.status) * dir;
        case "confirmations":
          return (a.confirmations - b.confirmations) * dir;
        default:
          return 0;
      }
    });

    // Apply sort preference from filter bar (if table sort is on date)
    if (tableSort.field === "date") {
      if (filters.sort === "newest") {
        items.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
      } else if (filters.sort === "oldest") {
        items.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
      } else if (filters.sort === "amount_high") {
        items.sort((a, b) => b.amountNanoerg - a.amountNanoerg);
      } else if (filters.sort === "amount_low") {
        items.sort((a, b) => a.amountNanoerg - b.amountNanoerg);
      }
    }

    return items;
  }, [transactions, filters, tableSort]);

  const handleTableSort = (field: SortField) => {
    setTableSort((prev) => ({
      field,
      dir: prev.field === field && prev.dir === "desc" ? "asc" : "desc",
    }));
  };

  // ── Gate: not authenticated ──
  if (!user) {
    return (
      <div className="max-w-5xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Transaction History</h1>
        <p className="text-surface-800/60 mb-8">
          View your on-chain inference payments, staking activity, and settlements.
        </p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">
            Sign in to view your transaction history
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

  // ── Default summary for loading state ──
  const defaultSummary: TxSummary = {
    totalSpent: 0,
    totalEarned: 0,
    totalTransactions: 0,
    pendingCount: 0,
    firstTxDate: null,
    lastTxDate: null,
  };

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold mb-1">Transaction History</h1>
          <p className="text-surface-800/60">
            On-chain payments, staking activity, and settlement records.
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
          <TxExport transactions={filteredTransactions} />
        </div>
      </div>

      <SuspenseWrap fallback={<TransactionsSkeleton />}>
      {/* Error display */}
      {error && (
        <div className="rounded-xl border border-danger-500/30 bg-danger-500/5 p-4 mb-6">
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-danger-500 shrink-0 mt-0.5">
              <circle cx="12" cy="12" r="10" />
              <line x1="15" y1="9" x2="9" y2="15" />
              <line x1="9" y1="9" x2="15" y2="15" />
            </svg>
            <div>
              <p className="text-sm font-medium text-danger-600">Failed to load transactions</p>
              <p className="text-xs text-surface-800/50 mt-0.5">{error.message}</p>
              <button
                onClick={load}
                className="mt-2 px-3 py-1 rounded-md text-xs font-medium bg-danger-500/10 text-danger-600 hover:bg-danger-500/20 transition-colors"
              >
                Retry
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Summary cards */}
      <TxSummaryCards summary={summary ?? defaultSummary} isLoading={loading && !summary} />

      {/* Filters */}
      <ErrorBoundary context="Transaction Filters">
        <TxFilters filters={filters} onChange={setFilters} />
      </ErrorBoundary>

      {/* Table */}
      <ErrorBoundary context="Transaction Table">
        <TxTable
          transactions={filteredTransactions}
          sortField={tableSort.field}
          sortDir={tableSort.dir}
          onSort={handleTableSort}
          isLoading={loading}
        />
      </ErrorBoundary>

      {/* Pagination */}
      {!loading && totalPages > 1 && (
        <div className="flex items-center justify-between mt-4">
          <button
            onClick={() => setPage((p) => Math.max(1, p - 1))}
            disabled={page <= 1}
            className="px-4 py-2 rounded-lg text-sm font-medium border border-surface-200 bg-surface-0 text-surface-800/70 hover:bg-surface-100 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Previous
          </button>
          <span className="text-sm text-surface-800/50">
            Page {page} of {totalPages}
          </span>
          <button
            onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
            disabled={page >= totalPages}
            className="px-4 py-2 rounded-lg text-sm font-medium border border-surface-200 bg-surface-0 text-surface-800/70 hover:bg-surface-100 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Next
          </button>
        </div>
      )}
      </SuspenseWrap>
    </div>
  );
}
