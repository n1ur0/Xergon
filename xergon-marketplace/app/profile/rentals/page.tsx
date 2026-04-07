"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  Clock,
  Cpu,
  MapPin,
  ChevronLeft,
  ChevronRight,
  Search,
  RefreshCw,
  XCircle,
  ArrowRight,
} from "lucide-react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type RentalStatus = "active" | "completed" | "cancelled" | "expired";

interface Rental {
  id: string;
  modelId: string;
  providerPk: string;
  providerRegion: string;
  status: RentalStatus;
  startedAt: string;
  endedAt?: string;
  durationHours: number;
  costNanoErg: number;
  tokensUsed: number;
}

interface RentalsResponse {
  rentals: Rental[];
  total: number;
}

const STATUS_CONFIG: Record<
  RentalStatus,
  { color: string; bg: string; label: string; animate?: boolean }
> = {
  active: { color: "text-emerald-700 dark:text-emerald-300", bg: "bg-emerald-100 dark:bg-emerald-900/30", label: "Active", animate: true },
  completed: { color: "text-blue-700 dark:text-blue-300", bg: "bg-blue-100 dark:bg-blue-900/30", label: "Completed" },
  cancelled: { color: "text-gray-600 dark:text-gray-400", bg: "bg-gray-100 dark:bg-gray-800/50", label: "Cancelled" },
  expired: { color: "text-orange-700 dark:text-orange-300", bg: "bg-orange-100 dark:bg-orange-900/30", label: "Expired" },
};

const PAGE_SIZE = 10;

function formatNanoErg(n: number): string {
  return `${(n / 1e9).toFixed(2)} ERG`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function truncatePk(pk: string): string {
  if (pk.length <= 20) return pk;
  return `${pk.slice(0, 10)}...${pk.slice(-6)}`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function RentalsPage() {
  const [rentals, setRentals] = useState<Rental[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [sort, setSort] = useState<string>("date_desc");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);

  const fetchRentals = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams({
        status: statusFilter,
        sort,
        limit: PAGE_SIZE.toString(),
        offset: (page * PAGE_SIZE).toString(),
      });
      const res = await fetch(`/api/user/rentals?${params}`);
      if (res.ok) {
        const data: RentalsResponse = await res.json();
        setRentals(data.rentals);
        setTotal(data.total);
      }
    } catch {
      // Silently fail
    } finally {
      setLoading(false);
    }
  }, [statusFilter, sort, page]);

  useEffect(() => {
    fetchRentals();
  }, [fetchRentals]);

  // Client-side search filter
  const filteredRentals = search
    ? rentals.filter(
        (r) =>
          r.modelId.toLowerCase().includes(search.toLowerCase()) ||
          r.providerPk.toLowerCase().includes(search.toLowerCase()) ||
          r.id.toLowerCase().includes(search.toLowerCase()),
      )
    : rentals;

  const totalPages = Math.ceil(total / PAGE_SIZE);
  const summarySpent = rentals.reduce((sum, r) => sum + r.costNanoErg, 0);
  const activeCount = rentals.filter((r) => r.status === "active").length;

  return (
    <div className="mx-auto max-w-4xl px-4 py-8 space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-0">
          Rental History
        </h1>
        <p className="mt-1 text-sm text-surface-800/50">
          View and manage your past and active model rentals.
        </p>
      </div>

      {/* Summary Bar */}
      <div className="grid grid-cols-3 gap-4">
        <div className="rounded-xl border border-surface-200 bg-white p-4 text-center dark:border-surface-700 dark:bg-surface-900">
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">{total}</p>
          <p className="text-xs text-surface-800/50">Total Rentals</p>
        </div>
        <div className="rounded-xl border border-surface-200 bg-white p-4 text-center dark:border-surface-700 dark:bg-surface-900">
          <p className="text-xl font-bold text-emerald-600">{activeCount}</p>
          <p className="text-xs text-surface-800/50">Active</p>
        </div>
        <div className="rounded-xl border border-surface-200 bg-white p-4 text-center dark:border-surface-700 dark:bg-surface-900">
          <p className="text-xl font-bold text-surface-900 dark:text-surface-0">{formatNanoErg(summarySpent)}</p>
          <p className="text-xs text-surface-800/50">Total Spent</p>
        </div>
      </div>

      {/* Filters Bar */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-wrap gap-2">
          {(["all", "active", "completed", "cancelled", "expired"] as const).map(
            (s) => (
              <button
                key={s}
                onClick={() => {
                  setStatusFilter(s);
                  setPage(0);
                }}
                className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                  statusFilter === s
                    ? "bg-brand-600 text-white"
                    : "border border-surface-300 text-surface-800/70 hover:bg-surface-100 dark:border-surface-600 dark:text-surface-300/70 dark:hover:bg-surface-800"
                }`}
              >
                {s.charAt(0).toUpperCase() + s.slice(1)}
              </button>
            ),
          )}
        </div>
        <div className="flex gap-2">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-surface-800/40" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search rentals..."
              className="w-48 rounded-lg border border-surface-300 bg-white py-1.5 pl-8 pr-3 text-sm dark:border-surface-600 dark:bg-surface-800"
            />
          </div>
          <select
            value={sort}
            onChange={(e) => {
              setSort(e.target.value);
              setPage(0);
            }}
            className="rounded-lg border border-surface-300 bg-white px-3 py-1.5 text-sm dark:border-surface-600 dark:bg-surface-800"
          >
            <option value="date_desc">Newest</option>
            <option value="date_asc">Oldest</option>
            <option value="cost_desc">Highest Cost</option>
          </select>
        </div>
      </div>

      {/* Rental Cards */}
      {loading ? (
        <div className="space-y-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="animate-pulse h-28 rounded-xl bg-surface-200 dark:bg-surface-800" />
          ))}
        </div>
      ) : filteredRentals.length === 0 ? (
        <div className="rounded-2xl border border-dashed border-surface-300 py-16 text-center dark:border-surface-600">
          <Clock className="mx-auto h-10 w-10 text-surface-800/20" />
          <p className="mt-3 text-surface-800/50">No rentals found.</p>
        </div>
      ) : (
        <div className="space-y-3">
          {filteredRentals.map((rental) => {
            const cfg = STATUS_CONFIG[rental.status];
            return (
              <div
                key={rental.id}
                className="rounded-xl border border-surface-200 bg-white p-4 dark:border-surface-700 dark:bg-surface-900"
              >
                <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  {/* Left: rental info */}
                  <div className="flex-1 space-y-1">
                    <div className="flex items-center gap-2">
                      <span
                        className={`inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium ${cfg.color} ${cfg.bg}`}
                      >
                        {cfg.animate && (
                          <span className="h-1.5 w-1.5 rounded-full bg-current animate-pulse" />
                        )}
                        {cfg.label}
                      </span>
                      <span className="text-xs font-mono text-surface-800/40">
                        {rental.id}
                      </span>
                    </div>
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
                      <span className="flex items-center gap-1 text-surface-800/70 dark:text-surface-300/70">
                        <Cpu className="h-3.5 w-3.5" />
                        {rental.modelId}
                      </span>
                      <span className="flex items-center gap-1 text-surface-800/50">
                        <MapPin className="h-3.5 w-3.5" />
                        {rental.providerRegion}
                      </span>
                      <span className="font-mono text-xs text-surface-800/40">
                        {truncatePk(rental.providerPk)}
                      </span>
                    </div>
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-surface-800/50">
                      <span className="flex items-center gap-1">
                        <Clock className="h-3 w-3" />
                        {formatDate(rental.startedAt)}
                        {rental.endedAt && ` → ${formatDate(rental.endedAt)}`}
                      </span>
                      <span>{rental.durationHours}h</span>
                      <span className="font-medium text-surface-900 dark:text-surface-0">
                        {formatNanoErg(rental.costNanoErg)}
                      </span>
                      <span>{formatNumber(rental.tokensUsed)} tokens</span>
                    </div>
                  </div>

                  {/* Right: actions */}
                  <div className="flex gap-2 shrink-0">
                    {rental.status === "active" && (
                      <>
                        <button className="inline-flex items-center gap-1 rounded-lg border border-surface-300 px-3 py-1.5 text-xs font-medium transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800">
                          <RefreshCw className="h-3 w-3" />
                          Extend
                        </button>
                        <button className="inline-flex items-center gap-1 rounded-lg border border-red-300 px-3 py-1.5 text-xs font-medium text-red-600 transition-colors hover:bg-red-50 dark:border-red-800 dark:hover:bg-red-900/20">
                          <XCircle className="h-3 w-3" />
                          Cancel
                        </button>
                      </>
                    )}
                    {rental.status === "completed" && (
                      <button className="inline-flex items-center gap-1 rounded-lg bg-brand-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-brand-700">
                        <ArrowRight className="h-3 w-3" />
                        Rent Again
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Pagination */}
      {!loading && totalPages > 1 && (
        <div className="flex items-center justify-center gap-2">
          <button
            disabled={page === 0}
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            className="inline-flex items-center gap-1 rounded-lg border border-surface-300 px-3 py-1.5 text-sm transition-colors hover:bg-surface-100 disabled:opacity-40 dark:border-surface-600 dark:hover:bg-surface-800"
          >
            <ChevronLeft className="h-4 w-4" />
            Previous
          </button>
          <span className="text-sm text-surface-800/50">
            Page {page + 1} of {totalPages}
          </span>
          <button
            disabled={page >= totalPages - 1}
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            className="inline-flex items-center gap-1 rounded-lg border border-surface-300 px-3 py-1.5 text-sm transition-colors hover:bg-surface-100 disabled:opacity-40 dark:border-surface-600 dark:hover:bg-surface-800"
          >
            Next
            <ChevronRight className="h-4 w-4" />
          </button>
        </div>
      )}

      {/* Back link */}
      <Link
        href="/profile"
        className="inline-flex items-center gap-1 text-sm text-brand-600 hover:underline"
      >
        <ChevronLeft className="h-4 w-4" />
        Back to Profile
      </Link>
    </div>
  );
}
