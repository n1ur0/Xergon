"use client";

import { useState, useEffect, useCallback } from "react";
import { cn } from "@/lib/utils";
import { SummaryCard } from "@/components/earnings/SummaryCard";
import { InvoiceCard, type Invoice, type InvoiceStatus } from "@/components/billing/InvoiceCard";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface BillingOverview {
  totalSpent: number;
  thisMonth: number;
  invoiceCount: number;
  creditsRemaining: number;
}

interface Transaction {
  id: string;
  date: string;
  model: string;
  provider: string;
  promptTokens: number;
  completionTokens: number;
  cost: number;
  status: "completed" | "failed" | "refunded";
}

interface UsageByModel {
  model: string;
  tokens: number;
  cost: number;
  requests: number;
}

interface SpendingPoint {
  date: string;
  amount: number;
}

interface BillingData {
  overview: BillingOverview;
  transactions: Transaction[];
  invoices: Invoice[];
  usageByModel: UsageByModel[];
  spendingChart: SpendingPoint[];
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function formatErg(amount: number): string {
  return amount.toFixed(4);
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(1) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return n.toString();
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatShortDate(dateStr: string): string {
  const d = new Date(dateStr + "T00:00:00");
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function truncateAddr(addr: string): string {
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
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
        <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-5">
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
// Spending Chart (SVG line chart)
// ---------------------------------------------------------------------------

function SpendingChart({ data }: { data: SpendingPoint[] }) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  if (!data.length) {
    return (
      <div className="flex items-center justify-center h-[240px] text-sm text-surface-800/40">
        No spending data available
      </div>
    );
  }

  const maxValue = Math.max(...data.map((d) => d.amount), 0.001);
  const width = 700;
  const height = 200;
  const padding = { top: 20, right: 20, bottom: 30, left: 50 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const points = data.map((d, i) => ({
    x: padding.left + (i / (data.length - 1)) * chartW,
    y: padding.top + chartH - (d.amount / maxValue) * chartH,
    ...d,
  }));

  const linePath = points
    .map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`)
    .join(" ");

  const areaPath = `${linePath} L ${points[points.length - 1].x} ${padding.top + chartH} L ${points[0].x} ${padding.top + chartH} Z`;

  return (
    <div className="relative">
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Grid lines */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac) => {
          const y = padding.top + chartH - frac * chartH;
          const val = (frac * maxValue).toFixed(2);
          return (
            <g key={frac}>
              <line x1={padding.left} y1={y} x2={width - padding.right} y2={y} stroke="currentColor" strokeOpacity={0.06} />
              <text x={padding.left - 8} y={y + 3} textAnchor="end" className="fill-surface-800/30 text-[10px]">
                {val}
              </text>
            </g>
          );
        })}

        {/* Area fill */}
        <path d={areaPath} fill="url(#spendGradient)" />
        <defs>
          <linearGradient id="spendGradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="currentColor" stopOpacity={0.15} />
            <stop offset="100%" stopColor="currentColor" stopOpacity={0.01} />
          </linearGradient>
        </defs>

        {/* Line */}
        <path d={linePath} fill="none" stroke="currentColor" strokeWidth={2} className="text-brand-500" />

        {/* Data points */}
        {points.map((p, i) => (
          <circle
            key={i}
            cx={p.x}
            cy={p.y}
            r={hoveredIndex === i ? 4 : 2}
            className={cn(
              "transition-all cursor-pointer",
              hoveredIndex === i ? "fill-brand-500" : "fill-brand-400/50",
            )}
            onMouseEnter={() => setHoveredIndex(i)}
            onMouseLeave={() => setHoveredIndex(null)}
          />
        ))}

        {/* Hover line */}
        {hoveredIndex !== null && points[hoveredIndex] && (
          <line
            x1={points[hoveredIndex].x}
            y1={padding.top}
            x2={points[hoveredIndex].x}
            y2={padding.top + chartH}
            stroke="currentColor"
            strokeOpacity={0.1}
            strokeDasharray="4 4"
          />
        )}

        {/* X-axis labels */}
        {points.map((p, i) =>
          i % 5 === 0 ? (
            <text key={i} x={p.x} y={height - 5} textAnchor="middle" className="fill-surface-800/30 text-[10px]">
              {formatShortDate(p.date)}
            </text>
          ) : null,
        )}
      </svg>

      {/* Tooltip */}
      {hoveredIndex !== null && points[hoveredIndex] && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-10 whitespace-nowrap rounded-lg border border-surface-200 bg-surface-0 px-3 py-1.5 text-xs shadow-lg pointer-events-none">
          <div className="font-semibold text-surface-900">
            {formatErg(points[hoveredIndex].amount)} ERG
          </div>
          <div className="text-surface-800/50">{formatShortDate(points[hoveredIndex].date)}</div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Usage breakdown bar chart
// ---------------------------------------------------------------------------

function UsageBreakdown({ data }: { data: UsageByModel[] }) {
  const maxCost = Math.max(...data.map((d) => d.cost), 0.001);

  return (
    <div className="space-y-3">
      {data.map((d) => {
        const pct = (d.cost / maxCost) * 100;
        return (
          <div key={d.model} className="group">
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs font-mono font-medium text-surface-900 truncate">
                {d.model}
              </span>
              <div className="flex items-center gap-3 text-xs text-surface-800/50">
                <span>{formatTokens(d.tokens)} tokens</span>
                <span className="font-medium text-surface-900">
                  {formatErg(d.cost)} ERG
                </span>
              </div>
            </div>
            <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
              <div
                className="h-full rounded-full bg-brand-500 transition-all group-hover:bg-brand-600"
                style={{ width: `${pct}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Transaction status badge
// ---------------------------------------------------------------------------

function TxStatusBadge({ status }: { status: string }) {
  const config: Record<string, { label: string; className: string }> = {
    completed: {
      label: "Completed",
      className: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    },
    failed: {
      label: "Failed",
      className: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
    },
    refunded: {
      label: "Refunded",
      className: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    },
  };
  const c = config[status] || config.completed;
  return (
    <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium", c.className)}>
      {c.label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Export CSV helper
// ---------------------------------------------------------------------------

function exportCSV(transactions: Transaction[], filename: string) {
  const headers = ["Date", "Model", "Provider", "Prompt Tokens", "Completion Tokens", "Cost (ERG)", "Status"];
  const rows = transactions.map((t) => [
    t.date,
    t.model,
    t.provider,
    t.promptTokens,
    t.completionTokens,
    t.cost.toFixed(6),
    t.status,
  ]);
  const csv = [headers.join(","), ...rows.map((r) => r.join(","))].join("\n");
  const blob = new Blob([csv], { type: "text/csv" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function BillingDashboard() {
  const [data, setData] = useState<BillingData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filterModel, setFilterModel] = useState<string>("all");
  const [filterProvider, setFilterProvider] = useState<string>("all");
  const [filterStatus, setFilterStatus] = useState<string>("all");

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const res = await fetch("/api/billing");
      if (!res.ok) throw new Error("Failed to load billing data");
      const json = await res.json();
      setData(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load billing data");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const filteredTransactions = data?.transactions.filter((t) => {
    if (filterModel !== "all" && t.model !== filterModel) return false;
    if (filterProvider !== "all" && t.provider !== filterProvider) return false;
    if (filterStatus !== "all" && t.status !== filterStatus) return false;
    return true;
  }) ?? [];

  const uniqueModels = [...new Set(data?.transactions.map((t) => t.model) ?? [])];
  const uniqueProviders = [...new Set(data?.transactions.map((t) => t.provider) ?? [])];

  const handleInvoiceDownload = (invoiceId: string) => {
    const invoice = data?.invoices.find((inv) => inv.id === invoiceId);
    if (!invoice) return;
    // Simulate PDF download by creating a text blob
    const content = `XERGON NETWORK INVOICE\nInvoice ID: ${invoice.id}\nAmount: ${formatErg(invoice.amount)} ERG\nDate: ${invoice.date}\nDue: ${invoice.dueDate}\nStatus: ${invoice.status}\n\n${invoice.description}`;
    const blob = new Blob([content], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `invoice-${invoice.id}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Billing Dashboard</h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Track spending, manage invoices, and monitor usage
          </p>
        </div>
        <div className="flex items-center gap-2 self-start">
          {data && (
            <button
              onClick={() => exportCSV(filteredTransactions, `xergon-billing-${new Date().toISOString().split("T")[0]}.csv`)}
              className="inline-flex items-center gap-2 rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-xs font-medium text-surface-800/60 transition-colors hover:bg-surface-100 hover:text-surface-900"
            >
              <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
              </svg>
              Export CSV
            </button>
          )}
        </div>
      </div>

      {/* Error */}
      {error && !isLoading && (
        <div className="mb-6 rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Overview cards */}
      <div className="mb-6">
        {isLoading ? (
          <StatsSkeleton />
        ) : data ? (
          <ErrorBoundary context="Billing Overview">
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
              <SummaryCard
                title="Total Spent"
                value={`${formatErg(data.overview.totalSpent)} ERG`}
                subtitle="All time"
                icon={
                  <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 18.75a60.07 60.07 0 0115.797 2.101c.727.198 1.453-.342 1.453-1.096V18.75M3.75 4.5v.75A.75.75 0 013 6h-.75m0 0v-.375c0-.621.504-1.125 1.125-1.125H20.25M2.25 6v9m18-10.5v.75c0 .414.336.75.75.75h.75m-1.5-1.5h.375c.621 0 1.125.504 1.125 1.125v9.75c0 .621-.504 1.125-1.125 1.125h-.375m1.5-1.5H21a.75.75 0 00-.75.75v.75m0 0H3.75m0 0h-.375a1.125 1.125 0 01-1.125-1.125V15m1.5 1.5v-.75A.75.75 0 003 15h-.75M15 10.5a3 3 0 11-6 0 3 3 0 016 0zm3 0h.008v.008H18V10.5zm-12 0h.008v.008H6V10.5z" />
                  </svg>
                }
              />
              <SummaryCard
                title="This Month"
                value={`${formatErg(data.overview.thisMonth)} ERG`}
                subtitle="Current billing cycle"
                trend="up"
                trendValue="+5.2%"
                icon={
                  <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
                  </svg>
                }
              />
              <SummaryCard
                title="Invoices"
                value={String(data.overview.invoiceCount)}
                subtitle={`${data.invoices.filter((i) => i.status === "pending" as InvoiceStatus).length} pending`}
                icon={
                  <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
                  </svg>
                }
              />
              <SummaryCard
                title="Credits"
                value={`${formatErg(data.overview.creditsRemaining)} ERG`}
                subtitle="Available balance"
                icon={
                  <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M21 11.25v8.25a1.5 1.5 0 01-1.5 1.5H5.25a1.5 1.5 0 01-1.5-1.5v-8.25M12 4.875A2.625 2.625 0 109.375 7.5H12m0-2.625V7.5m0-2.625A2.625 2.625 0 1114.625 7.5H12m0 0V21m-8.625-9.75h18c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125h-18c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z" />
                  </svg>
                }
              />
            </div>
          </ErrorBoundary>
        ) : null}
      </div>

      {/* Credit balance + top-up */}
      {data && (
        <div className="mb-6 rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium text-surface-800/60">Credit Balance</div>
              <div className="text-3xl font-bold text-surface-900 mt-1">
                {formatErg(data.overview.creditsRemaining)} <span className="text-lg font-normal text-surface-800/40">ERG</span>
              </div>
            </div>
            <button className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-brand-700">
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
              </svg>
              Top Up Credits
            </button>
          </div>
        </div>
      )}

      {/* Spending chart + Usage breakdown */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        <div className="lg:col-span-2">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-5 w-40 mb-4" />
              <SkeletonPulse className="h-[240px] w-full" />
            </div>
          ) : data ? (
            <ErrorBoundary context="Spending Chart">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-base font-semibold text-surface-900">Daily Spending</h2>
                  <span className="rounded-lg bg-surface-100 px-3 py-1 text-xs font-medium text-surface-800/50">
                    Last 30 days
                  </span>
                </div>
                <SpendingChart data={data.spendingChart} />
              </div>
            </ErrorBoundary>
          ) : null}
        </div>

        <div>
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-5 w-36 mb-4" />
              <div className="space-y-4">
                {Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="space-y-2">
                    <SkeletonPulse className="h-3 w-24" />
                    <SkeletonPulse className="h-2 w-full rounded-full" />
                  </div>
                ))}
              </div>
            </div>
          ) : data ? (
            <ErrorBoundary context="Usage Breakdown">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
                <h2 className="text-base font-semibold text-surface-900 mb-4">Usage by Model</h2>
                <UsageBreakdown data={data.usageByModel} />
              </div>
            </ErrorBoundary>
          ) : null}
        </div>
      </div>

      {/* Filters */}
      {data && (
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <select
            value={filterModel}
            onChange={(e) => setFilterModel(e.target.value)}
            className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs font-medium text-surface-800/60"
          >
            <option value="all">All Models</option>
            {uniqueModels.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
          <select
            value={filterProvider}
            onChange={(e) => setFilterProvider(e.target.value)}
            className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs font-medium text-surface-800/60"
          >
            <option value="all">All Providers</option>
            {uniqueProviders.map((p) => (
              <option key={p} value={p}>{truncateAddr(p)}</option>
            ))}
          </select>
          <select
            value={filterStatus}
            onChange={(e) => setFilterStatus(e.target.value)}
            className="rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs font-medium text-surface-800/60"
          >
            <option value="all">All Statuses</option>
            <option value="completed">Completed</option>
            <option value="failed">Failed</option>
            <option value="refunded">Refunded</option>
          </select>
        </div>
      )}

      {/* Transactions table */}
      {isLoading ? (
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden mb-6">
          <div className="px-5 py-4 border-b border-surface-100">
            <SkeletonPulse className="h-5 w-40" />
          </div>
          <div className="space-y-0">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="flex items-center gap-4 px-5 py-3 border-b border-surface-50">
                <SkeletonPulse className="h-4 w-24" />
                <div className="flex-1" />
                <SkeletonPulse className="h-4 w-20" />
                <SkeletonPulse className="h-4 w-16" />
              </div>
            ))}
          </div>
        </div>
      ) : data ? (
        <ErrorBoundary context="Transactions Table">
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden shadow-sm mb-6">
            <div className="px-5 py-4 border-b border-surface-100">
              <h2 className="text-base font-semibold text-surface-900">
                Recent Transactions
              </h2>
              <p className="text-xs text-surface-800/40 mt-0.5">
                Showing {filteredTransactions.length} of {data.transactions.length} transactions
              </p>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-surface-100">
                    <th className="text-left px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">Date</th>
                    <th className="text-left px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">Model</th>
                    <th className="text-left px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider hidden sm:table-cell">Provider</th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider hidden md:table-cell">Tokens</th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">Cost</th>
                    <th className="text-right px-5 py-3 text-xs font-medium text-surface-800/50 uppercase tracking-wider">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredTransactions.map((tx) => (
                    <tr key={tx.id} className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors">
                      <td className="px-5 py-3 text-xs text-surface-800/50">{formatDate(tx.date)}</td>
                      <td className="px-5 py-3 font-mono text-xs font-medium text-surface-900">{tx.model}</td>
                      <td className="px-5 py-3 text-xs text-surface-800/50 hidden sm:table-cell">{truncateAddr(tx.provider)}</td>
                      <td className="px-5 py-3 text-right text-xs text-surface-800/60 hidden md:table-cell">
                        {formatTokens(tx.promptTokens + tx.completionTokens)}
                      </td>
                      <td className="px-5 py-3 text-right text-xs font-medium text-surface-900">
                        {formatErg(tx.cost)} ERG
                      </td>
                      <td className="px-5 py-3 text-right">
                        <TxStatusBadge status={tx.status} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </ErrorBoundary>
      ) : null}

      {/* Invoices list */}
      {isLoading ? (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <SkeletonPulse className="h-5 w-32 mb-4" />
          <div className="space-y-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <SkeletonPulse key={i} className="h-16 w-full" />
            ))}
          </div>
        </div>
      ) : data ? (
        <ErrorBoundary context="Invoices List">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm">
            <h2 className="text-base font-semibold text-surface-900 mb-4">Invoices</h2>
            {data.invoices.length === 0 ? (
              <div className="text-sm text-surface-800/40 py-8 text-center">No invoices yet</div>
            ) : (
              <div className="space-y-3">
                {data.invoices.map((inv) => (
                  <InvoiceCard key={inv.id} invoice={inv} onDownload={handleInvoiceDownload} />
                ))}
              </div>
            )}
          </div>
        </ErrorBoundary>
      ) : null}
    </div>
  );
}
