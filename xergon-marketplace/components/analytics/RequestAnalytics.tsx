"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface InferenceRequest {
  id: string;
  timestamp: string;
  model: string;
  provider: string;
  providerPk: string;
  status: "success" | "error" | "timeout" | "rate_limited";
  latencyMs: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  costNanoerg: number;
  region: string;
  errorMessage?: string;
  requestBody?: string;
  responseBody?: string;
}

interface RequestSummary {
  totalRequests: number;
  successCount: number;
  errorCount: number;
  timeoutCount: number;
  rateLimitedCount: number;
  successRate: number;
  avgLatencyMs: number;
  p50LatencyMs: number;
  p90LatencyMs: number;
  p99LatencyMs: number;
  totalTokens: number;
  totalCostNanoerg: number;
  avgTokensPerRequest: number;
  uniqueModels: number;
  uniqueProviders: number;
}

interface Pagination {
  page: number;
  pageSize: number;
  total: number;
  totalPages: number;
}

interface Filters {
  availableModels: string[];
  availableProviders: string[];
  availableRegions: string[];
  availableStatuses: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  if (erg >= 1_000) return `${(erg / 1_000).toFixed(1)}K ERG`;
  return `${erg.toFixed(2)} ERG`;
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatLatency(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
  return `${ms}ms`;
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

const STATUS_CONFIG: Record<string, { label: string; className: string }> = {
  success: {
    label: "Success",
    className: "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-400",
  },
  error: {
    label: "Error",
    className: "bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-400",
  },
  timeout: {
    label: "Timeout",
    className: "bg-yellow-50 text-yellow-700 dark:bg-yellow-950/30 dark:text-yellow-400",
  },
  rate_limited: {
    label: "Rate Limited",
    className: "bg-orange-50 text-orange-700 dark:bg-orange-950/30 dark:text-orange-400",
  },
};

function StatusBadge({ status }: { status: string }) {
  const config = STATUS_CONFIG[status] ?? STATUS_CONFIG.error;
  return (
    <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold", config.className)}>
      {config.label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={cn("skeleton-shimmer rounded-lg", className)} />;
}

function LoadingSkeleton() {
  return (
    <div className="max-w-7xl mx-auto px-4 py-8 space-y-6">
      <div className="flex items-center justify-between">
        <SkeletonPulse className="h-8 w-56" />
        <div className="flex gap-2">
          <SkeletonPulse className="h-9 w-28" />
          <SkeletonPulse className="h-9 w-28" />
        </div>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
        {Array.from({ length: 6 }).map((_, i) => (
          <SkeletonPulse key={i} className="h-24" />
        ))}
      </div>
      <div className="flex gap-3">
        <SkeletonPulse className="h-10 w-40" />
        <SkeletonPulse className="h-10 w-40" />
        <SkeletonPulse className="h-10 w-40" />
        <SkeletonPulse className="h-10 w-48" />
      </div>
      <SkeletonPulse className="h-[600px]" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stats card
// ---------------------------------------------------------------------------

function StatCard({
  label,
  value,
  sub,
  icon,
}: {
  label: string;
  value: string;
  sub?: string;
  icon: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 transition-all hover:shadow-md">
      <div className="flex items-center gap-2 mb-2">
        <div className="rounded-lg bg-brand-50 p-1.5 text-brand-600 dark:bg-brand-950/30">
          {icon}
        </div>
        <span className="text-xs text-surface-800/50 font-medium">{label}</span>
      </div>
      <div className="text-xl font-bold text-surface-900">{value}</div>
      {sub && <div className="text-[11px] text-surface-800/40 mt-0.5">{sub}</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Expandable row
// ---------------------------------------------------------------------------

function ExpandedRow({ req }: { req: InferenceRequest }) {
  return (
    <tr className="bg-surface-50/50 dark:bg-surface-900/20">
      <td colSpan={8} className="px-4 py-3">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
          <div>
            <div className="font-semibold text-surface-700 mb-1">Request Details</div>
            <div className="space-y-1 text-surface-600">
              <div><span className="text-surface-800/50">ID:</span> {req.id}</div>
              <div><span className="text-surface-800/50">Provider PK:</span> {req.providerPk}</div>
              <div><span className="text-surface-800/50">Region:</span> {req.region}</div>
              <div><span className="text-surface-800/50">Timestamp:</span> {new Date(req.timestamp).toISOString()}</div>
            </div>
          </div>
          <div>
            <div className="font-semibold text-surface-700 mb-1">Token & Cost Breakdown</div>
            <div className="space-y-1 text-surface-600">
              <div><span className="text-surface-800/50">Input Tokens:</span> {req.inputTokens.toLocaleString()}</div>
              <div><span className="text-surface-800/50">Output Tokens:</span> {req.outputTokens.toLocaleString()}</div>
              <div><span className="text-surface-800/50">Total Tokens:</span> {req.totalTokens.toLocaleString()}</div>
              <div><span className="text-surface-800/50">Cost:</span> {nanoergToErg(req.costNanoerg)}</div>
            </div>
          </div>
          {req.errorMessage && (
            <div className="md:col-span-2">
              <div className="font-semibold text-surface-700 mb-1">Error</div>
              <div className="rounded-lg bg-red-50 dark:bg-red-950/20 px-3 py-2 text-red-700 dark:text-red-400 font-mono text-[11px]">
                {req.errorMessage}
              </div>
            </div>
          )}
          {req.requestBody && (
            <div>
              <div className="font-semibold text-surface-700 mb-1">Request Body</div>
              <pre className="rounded-lg bg-surface-100 dark:bg-surface-800 px-3 py-2 text-[10px] text-surface-600 overflow-x-auto max-h-32">
                {req.requestBody}
              </pre>
            </div>
          )}
          {req.responseBody && (
            <div>
              <div className="font-semibold text-surface-700 mb-1">Response Body</div>
              <pre className="rounded-lg bg-surface-100 dark:bg-surface-800 px-3 py-2 text-[10px] text-surface-600 overflow-x-auto max-h-32">
                {req.responseBody}
              </pre>
            </div>
          )}
        </div>
      </td>
    </tr>
  );
}

// ---------------------------------------------------------------------------
// CSV export
// ---------------------------------------------------------------------------

function downloadCSV(requests: InferenceRequest[], summary: RequestSummary) {
  const headers = ["ID", "Timestamp", "Model", "Provider", "Status", "LatencyMs", "InputTokens", "OutputTokens", "TotalTokens", "CostNanoerg", "Region"];
  const rows = requests.map((r) => [
    r.id,
    r.timestamp,
    r.model,
    r.provider,
    r.status,
    r.latencyMs,
    r.inputTokens,
    r.outputTokens,
    r.totalTokens,
    r.costNanoerg,
    r.region,
  ]);
  const csvContent = [headers.join(","), ...rows.map((r) => r.map((v) => `"${v}"`).join(","))].join("\n");
  const blob = new Blob([csvContent], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `xergon-requests-${new Date().toISOString().split("T")[0]}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function RequestAnalytics() {
  // Data state
  const [requests, setRequests] = useState<InferenceRequest[]>([]);
  const [summary, setSummary] = useState<RequestSummary | null>(null);
  const [pagination, setPagination] = useState<Pagination>({ page: 1, pageSize: 20, total: 0, totalPages: 0 });
  const [filters, setFilters] = useState<Filters | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Filter state
  const [modelFilter, setModelFilter] = useState("");
  const [providerFilter, setProviderFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");

  // UI state
  const [expandedRow, setExpandedRow] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(false);

  // Load data
  const loadData = useCallback(async () => {
    try {
      setError(null);
      const params = new URLSearchParams({
        page: String(pagination.page),
        pageSize: String(pagination.pageSize),
      });
      if (modelFilter) params.set("model", modelFilter);
      if (providerFilter) params.set("provider", providerFilter);
      if (statusFilter) params.set("status", statusFilter);
      if (dateFrom) params.set("startDate", dateFrom);
      if (dateTo) params.set("endDate", dateTo);

      const [listRes, summaryRes] = await Promise.all([
        fetch(`/api/analytics/requests?${params}`),
        fetch("/api/analytics/requests?action=summary"),
      ]);

      if (!listRes.ok || !summaryRes.ok) throw new Error("Failed to fetch data");

      const listData = await listRes.json();
      const summaryData = await summaryRes.json();

      setRequests(listData.requests);
      setPagination(listData.pagination);
      setFilters(listData.filters);
      setSummary(summaryData);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load requests");
    } finally {
      setIsLoading(false);
    }
  }, [pagination.page, pagination.pageSize, modelFilter, providerFilter, statusFilter, dateFrom, dateTo]);

  useEffect(() => {
    setIsLoading(true);
    loadData();
  }, [loadData]);

  // Auto-refresh
  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(() => {
      loadData();
    }, 15000);
    return () => clearInterval(interval);
  }, [autoRefresh, loadData]);

  // Reset page on filter change
  const handleFilterChange = useCallback(
    (setter: (v: string) => void) =>
      (e: React.ChangeEvent<HTMLSelectElement>) => {
        setter(e.target.value);
        setPagination((p) => ({ ...p, page: 1 }));
      },
    []
  );

  const handleDateChange = useCallback(
    (setter: (v: string) => void) =>
      (e: React.ChangeEvent<HTMLInputElement>) => {
        setter(e.target.value);
        setPagination((p) => ({ ...p, page: 1 }));
      },
    []
  );

  // Stats icons (inline SVG)
  const IconRequests = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
    </svg>
  );
  const IconSuccess = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
      <polyline points="22 4 12 14.01 9 11.01" />
    </svg>
  );
  const IconLatency = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  );
  const IconTokens = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
      <polyline points="22 6 12 13 2 6" />
    </svg>
  );
  const IconCost = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <path d="M16 8h-6a2 2 0 000 7h4a2 2 0 010 7H8" />
      <path d="M12 18V6" />
    </svg>
  );
  const IconP90 = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <line x1="18" y1="20" x2="18" y2="10" />
      <line x1="12" y1="20" x2="12" y2="4" />
      <line x1="6" y1="20" x2="6" y2="14" />
    </svg>
  );

  if (isLoading) return <LoadingSkeleton />;

  if (error) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8">
        <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-7xl mx-auto px-4 py-8 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Request Analytics</h1>
          <p className="text-sm text-surface-800/50 mt-1">Monitor and analyze inference request performance</p>
        </div>
        <div className="flex items-center gap-2">
          {/* Auto-refresh toggle */}
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={cn(
              "flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs font-medium transition-colors",
              autoRefresh
                ? "border-brand-300 bg-brand-50 text-brand-700 dark:border-brand-700 dark:bg-brand-950/30 dark:text-brand-400"
                : "border-surface-200 bg-surface-0 text-surface-600 hover:bg-surface-50"
            )}
          >
            <svg className={cn("w-3.5 h-3.5", autoRefresh && "animate-spin")} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="23 4 23 10 17 10" />
              <path d="M20.49 15a9 9 0 11-2.12-9.36L23 10" />
            </svg>
            Auto-refresh
          </button>
          {/* CSV export */}
          <button
            onClick={() => summary && downloadCSV(requests, summary)}
            className="flex items-center gap-1.5 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs font-medium text-surface-600 hover:bg-surface-50 transition-colors"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
              <polyline points="7 10 12 15 17 10" />
              <line x1="12" y1="15" x2="12" y2="3" />
            </svg>
            Export CSV
          </button>
        </div>
      </div>

      {/* Stats cards */}
      {summary && (
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
          <StatCard label="Total Requests" value={formatNumber(summary.totalRequests)} sub={`${summary.uniqueModels} models, ${summary.uniqueProviders} providers`} icon={IconRequests} />
          <StatCard label="Success Rate" value={`${summary.successRate.toFixed(1)}%`} sub={`${summary.errorCount} errors, ${summary.timeoutCount} timeouts`} icon={IconSuccess} />
          <StatCard label="Avg Latency" value={formatLatency(summary.avgLatencyMs)} sub={`P50: ${formatLatency(summary.p50LatencyMs)}`} icon={IconLatency} />
          <StatCard label="P90 Latency" value={formatLatency(summary.p90LatencyMs)} sub={`P99: ${formatLatency(summary.p99LatencyMs)}`} icon={IconP90} />
          <StatCard label="Total Tokens" value={formatNumber(summary.totalTokens)} sub={`Avg ${summary.avgTokensPerRequest} per request`} icon={IconTokens} />
          <StatCard label="Total Cost" value={nanoergToErg(summary.totalCostNanoerg)} icon={IconCost} />
        </div>
      )}

      {/* Filters bar */}
      <div className="flex flex-wrap items-center gap-3">
        <select
          value={modelFilter}
          onChange={handleFilterChange(setModelFilter)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        >
          <option value="">All Models</option>
          {filters?.availableModels.map((m) => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>

        <select
          value={providerFilter}
          onChange={handleFilterChange(setProviderFilter)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        >
          <option value="">All Providers</option>
          {filters?.availableProviders.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>

        <select
          value={statusFilter}
          onChange={handleFilterChange(setStatusFilter)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        >
          <option value="">All Statuses</option>
          {filters?.availableStatuses.map((s) => (
            <option key={s} value={s}>{s.replace("_", " ").replace(/\b\w/g, (c) => c.toUpperCase())}</option>
          ))}
        </select>

        <input
          type="date"
          value={dateFrom}
          onChange={handleDateChange(setDateFrom)}
          placeholder="From"
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        />

        <input
          type="date"
          value={dateTo}
          onChange={handleDateChange(setDateTo)}
          placeholder="To"
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        />

        {/* Page size selector */}
        <select
          value={pagination.pageSize}
          onChange={(e) => setPagination({ ...pagination, page: 1, pageSize: Number(e.target.value) })}
          className="ml-auto rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-xs text-surface-700 focus:border-brand-400 focus:outline-none focus:ring-1 focus:ring-brand-400"
        >
          {[10, 20, 50, 100].map((s) => (
            <option key={s} value={s}>{s} per page</option>
          ))}
        </select>
      </div>

      {/* Requests table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-surface-200 bg-surface-50">
                <th className="px-4 py-3 text-left font-semibold text-surface-700 w-8"></th>
                <th className="px-4 py-3 text-left font-semibold text-surface-700">ID</th>
                <th className="px-4 py-3 text-left font-semibold text-surface-700">Model</th>
                <th className="px-4 py-3 text-left font-semibold text-surface-700">Provider</th>
                <th className="px-4 py-3 text-left font-semibold text-surface-700">Status</th>
                <th className="px-4 py-3 text-right font-semibold text-surface-700">Latency</th>
                <th className="px-4 py-3 text-right font-semibold text-surface-700">Tokens</th>
                <th className="px-4 py-3 text-right font-semibold text-surface-700">Cost</th>
                <th className="px-4 py-3 text-left font-semibold text-surface-700">Time</th>
              </tr>
            </thead>
            <tbody>
              {requests.length === 0 ? (
                <tr>
                  <td colSpan={9} className="px-4 py-12 text-center text-surface-800/40">
                    No requests found matching filters.
                  </td>
                </tr>
              ) : (
                requests.map((req) => (
                  <tbody key={req.id}>
                    <tr
                      className="border-b border-surface-100 hover:bg-surface-50/80 cursor-pointer transition-colors"
                      onClick={() => setExpandedRow(expandedRow === req.id ? null : req.id)}
                    >
                      <td className="px-4 py-3">
                        <svg
                          className={cn("w-3.5 h-3.5 text-surface-400 transition-transform", expandedRow === req.id && "rotate-90")}
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          <polyline points="9 18 15 12 9 6" />
                        </svg>
                      </td>
                      <td className="px-4 py-3 font-mono text-surface-600 max-w-[140px] truncate">{req.id}</td>
                      <td className="px-4 py-3 text-surface-800 max-w-[180px] truncate">{req.model}</td>
                      <td className="px-4 py-3 text-surface-600">{req.provider}</td>
                      <td className="px-4 py-3"><StatusBadge status={req.status} /></td>
                      <td className={cn(
                        "px-4 py-3 text-right font-mono",
                        req.latencyMs > 5000 ? "text-red-500" : req.latencyMs > 2000 ? "text-yellow-600" : "text-surface-700"
                      )}>
                        {formatLatency(req.latencyMs)}
                      </td>
                      <td className="px-4 py-3 text-right text-surface-700">{formatNumber(req.totalTokens)}</td>
                      <td className="px-4 py-3 text-right text-surface-700">{nanoergToErg(req.costNanoerg)}</td>
                      <td className="px-4 py-3 text-surface-600 whitespace-nowrap">{formatTimestamp(req.timestamp)}</td>
                    </tr>
                    {expandedRow === req.id && <ExpandedRow req={req} />}
                  </tbody>
                ))
              )}
            </tbody>
          </table>
        </div>

        {/* Pagination */}
        <div className="flex items-center justify-between border-t border-surface-200 px-4 py-3">
          <div className="text-xs text-surface-800/50">
            Showing {((pagination.page - 1) * pagination.pageSize) + 1}-{Math.min(pagination.page * pagination.pageSize, pagination.total)} of {pagination.total}
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setPagination({ ...pagination, page: 1 })}
              disabled={pagination.page <= 1}
              className="rounded-md border border-surface-200 px-2 py-1 text-xs text-surface-600 disabled:opacity-30 hover:bg-surface-50 transition-colors"
            >
              First
            </button>
            <button
              onClick={() => setPagination({ ...pagination, page: pagination.page - 1 })}
              disabled={pagination.page <= 1}
              className="rounded-md border border-surface-200 px-2 py-1 text-xs text-surface-600 disabled:opacity-30 hover:bg-surface-50 transition-colors"
            >
              Prev
            </button>
            <span className="px-3 py-1 text-xs font-medium text-surface-700">
              {pagination.page} / {pagination.totalPages}
            </span>
            <button
              onClick={() => setPagination({ ...pagination, page: pagination.page + 1 })}
              disabled={pagination.page >= pagination.totalPages}
              className="rounded-md border border-surface-200 px-2 py-1 text-xs text-surface-600 disabled:opacity-30 hover:bg-surface-50 transition-colors"
            >
              Next
            </button>
            <button
              onClick={() => setPagination({ ...pagination, page: pagination.totalPages })}
              disabled={pagination.page >= pagination.totalPages}
              className="rounded-md border border-surface-200 px-2 py-1 text-xs text-surface-600 disabled:opacity-30 hover:bg-surface-50 transition-colors"
            >
              Last
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
