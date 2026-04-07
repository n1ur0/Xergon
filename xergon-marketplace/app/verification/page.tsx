"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { fetchProviders, filterProviders, extractModels, type ProviderInfo } from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type VerificationStatus = "verified" | "unverified" | "pending";

interface VerificationCriteria {
  uptimeOk: boolean;
  ponwScoreOk: boolean;
  onChainBoxExists: boolean;
  validEndpoint: boolean;
}

interface VerificationRecord {
  providerId: string;
  provider: ProviderInfo;
  status: VerificationStatus;
  criteria: VerificationCriteria;
  ponwScore: number;
  lastVerified: string | null;
  verifiedBy: string | null;
  details: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function computeCriteria(p: ProviderInfo, ponwScore: number): VerificationCriteria {
  return {
    uptimeOk: p.uptime > 95,
    ponwScoreOk: ponwScore > 800,
    onChainBoxExists: !!p.ergoAddress,
    validEndpoint: p.status === "online",
  };
}

function determineStatus(criteria: VerificationCriteria): VerificationStatus {
  const allMet = Object.values(criteria).every(Boolean);
  const anyMet = Object.values(criteria).some(Boolean);
  if (allMet) return "verified";
  if (anyMet) return "pending";
  return "unverified";
}

function truncateAddr(addr: string, len = 8): string {
  if (addr.length <= len * 2 + 3) return addr;
  return `${addr.slice(0, len)}...${addr.slice(-len)}`;
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: VerificationStatus }) {
  const config = {
    verified: { bg: "bg-emerald-50 dark:bg-emerald-950/30", text: "text-emerald-700 dark:text-emerald-400", label: "Verified" },
    pending: { bg: "bg-amber-50 dark:bg-amber-950/30", text: "text-amber-700 dark:text-amber-400", label: "Pending" },
    unverified: { bg: "bg-red-50 dark:bg-red-950/30", text: "text-red-700 dark:text-red-400", label: "Unverified" },
  };
  const c = config[status];
  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium ${c.bg} ${c.text}`}>
      {status === "verified" && (
        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      )}
      {status === "pending" && (
        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      )}
      {status === "unverified" && (
        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="10" />
          <line x1="15" y1="9" x2="9" y2="15" />
          <line x1="9" y1="9" x2="15" y2="15" />
        </svg>
      )}
      {c.label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Criteria check indicator
// ---------------------------------------------------------------------------

function CriteriaCheck({ label, passed }: { label: string; passed: boolean }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className={`flex-shrink-0 w-4 h-4 rounded-full flex items-center justify-center ${passed ? "bg-emerald-100 text-emerald-600 dark:bg-emerald-900/40 dark:text-emerald-400" : "bg-red-100 text-red-500 dark:bg-red-900/40 dark:text-red-400"}`}>
        {passed ? (
          <svg className="w-2.5 h-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        ) : (
          <svg className="w-2.5 h-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        )}
      </span>
      <span className={passed ? "text-surface-800/70" : "text-surface-800/40"}>{label}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function LoadingSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8 space-y-6 animate-pulse">
      <div className="flex items-center justify-between">
        <div className="h-8 w-56 rounded-lg bg-surface-200" />
        <div className="h-8 w-48 rounded-lg bg-surface-200" />
      </div>
      <div className="grid grid-cols-3 gap-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="h-10 rounded-lg bg-surface-200" />
        ))}
      </div>
      <div className="space-y-4">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="h-32 rounded-xl border border-surface-200 bg-surface-0 p-5">
            <div className="h-5 w-40 bg-surface-200 rounded mb-3" />
            <div className="space-y-2">
              {Array.from({ length: 4 }).map((_, j) => (
                <div key={j} className="h-3 w-full bg-surface-100 rounded" />
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function VerificationPage() {
  const [records, setRecords] = useState<VerificationRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filterStatus, setFilterStatus] = useState<VerificationStatus | "all">("all");
  const [filterRegion, setFilterRegion] = useState("all");
  const [filterModel, setFilterModel] = useState("all");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  // Load providers and compute verification
  const loadData = useCallback(async () => {
    try {
      setError(null);
      const res = await fetchProviders();
      const verificationRecords: VerificationRecord[] = res.providers.map((p) => {
        // Simulate PoNW score (in real impl, fetch from chain / API)
        const ponwScore = Math.floor(Math.random() * 600) + 400 + (p.aiPoints > 50 ? 200 : 0);
        const criteria = computeCriteria(p, ponwScore);
        const status = determineStatus(criteria);
        return {
          providerId: p.endpoint,
          provider: p,
          status,
          criteria,
          ponwScore,
          lastVerified: p.lastSeen || null,
          verifiedBy: null,
          details: buildDetailsText(criteria, ponwScore, p),
        };
      });
      setRecords(verificationRecords);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load providers");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Available filters
  const availableModels = useMemo(() => extractModels(records.map((r) => r.provider)), [records]);
  const availableRegions = useMemo(() => {
    const set = new Set(records.map((r) => r.provider.region));
    return Array.from(set).sort();
  }, [records]);

  // Filtered records
  const filteredRecords = useMemo(() => {
    let result = records;
    if (filterStatus !== "all") result = result.filter((r) => r.status === filterStatus);
    if (filterRegion !== "all") result = result.filter((r) => r.provider.region === filterRegion);
    if (filterModel !== "all") result = result.filter((r) => r.provider.models.includes(filterModel));
    return result;
  }, [records, filterStatus, filterRegion, filterModel]);

  // Summary counts
  const counts = useMemo(() => ({
    total: records.length,
    verified: records.filter((r) => r.status === "verified").length,
    pending: records.filter((r) => r.status === "pending").length,
    unverified: records.filter((r) => r.status === "unverified").length,
  }), [records]);

  // Admin actions
  const handleVerify = useCallback(async (providerId: string) => {
    setActionLoading(providerId);
    try {
      await new Promise((r) => setTimeout(r, 800)); // simulate API call
      setRecords((prev) =>
        prev.map((rec) =>
          rec.providerId === providerId
            ? {
                ...rec,
                status: "verified" as VerificationStatus,
                criteria: { uptimeOk: true, ponwScoreOk: true, onChainBoxExists: true, validEndpoint: true },
                lastVerified: new Date().toISOString(),
                verifiedBy: "admin",
                details: "Manually verified by admin",
              }
            : rec,
        ),
      );
    } finally {
      setActionLoading(null);
    }
  }, []);

  const handleUnverify = useCallback(async (providerId: string) => {
    setActionLoading(providerId);
    try {
      await new Promise((r) => setTimeout(r, 800));
      setRecords((prev) =>
        prev.map((rec) =>
          rec.providerId === providerId
            ? { ...rec, status: "unverified" as VerificationStatus, lastVerified: new Date().toISOString(), verifiedBy: "admin" }
            : rec,
        ),
      );
    } finally {
      setActionLoading(null);
    }
  }, []);

  const handleReverify = useCallback(async (providerId: string) => {
    setActionLoading(providerId);
    try {
      await new Promise((r) => setTimeout(r, 1200));
      setRecords((prev) =>
        prev.map((rec) => {
          if (rec.providerId !== providerId) return rec;
          const newPonw = Math.floor(Math.random() * 400) + 600;
          const criteria = computeCriteria(rec.provider, newPonw);
          return {
            ...rec,
            ponwScore: newPonw,
            criteria,
            status: determineStatus(criteria),
            lastVerified: new Date().toISOString(),
            verifiedBy: "system",
            details: buildDetailsText(criteria, newPonw, rec.provider),
          };
        }),
      );
    } finally {
      setActionLoading(null);
    }
  }, []);

  if (loading) return <LoadingSkeleton />;

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Provider Verification</h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Review and manage provider verification status on the Xergon network
          </p>
        </div>
        <button
          type="button"
          onClick={loadData}
          className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-surface-200 bg-surface-0 text-sm font-medium text-surface-800/70 hover:bg-surface-50 transition-colors"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 4 23 10 17 10" />
            <path d="M20.49 15a9 9 0 11-2.12-9.36L23 10" />
          </svg>
          Refresh
        </button>
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <SummaryCard label="Total Providers" value={counts.total} color="bg-surface-100 dark:bg-surface-800" />
        <SummaryCard label="Verified" value={counts.verified} color="bg-emerald-100 dark:bg-emerald-900/30" textColor="text-emerald-700 dark:text-emerald-400" />
        <SummaryCard label="Pending" value={counts.pending} color="bg-amber-100 dark:bg-amber-900/30" textColor="text-amber-700 dark:text-amber-400" />
        <SummaryCard label="Unverified" value={counts.unverified} color="bg-red-100 dark:bg-red-900/30" textColor="text-red-700 dark:text-red-400" />
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-3 mb-6">
        <select
          value={filterStatus}
          onChange={(e) => setFilterStatus(e.target.value as VerificationStatus | "all")}
          className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          <option value="all">All Statuses</option>
          <option value="verified">Verified</option>
          <option value="pending">Pending</option>
          <option value="unverified">Unverified</option>
        </select>
        <select
          value={filterRegion}
          onChange={(e) => setFilterRegion(e.target.value)}
          className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          <option value="all">All Regions</option>
          {availableRegions.map((r) => (
            <option key={r} value={r}>{r}</option>
          ))}
        </select>
        <select
          value={filterModel}
          onChange={(e) => setFilterModel(e.target.value)}
          className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-800 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          <option value="all">All Models</option>
          {availableModels.map((m) => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>
        <span className="text-xs text-surface-800/40 self-center">
          {filteredRecords.length} of {records.length} providers
        </span>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400 mb-6">
          {error}
        </div>
      )}

      {/* Provider list */}
      <div className="space-y-3">
        {filteredRecords.length === 0 && (
          <div className="text-center py-16 text-surface-800/40">
            <svg className="w-12 h-12 mx-auto mb-3 text-surface-300" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p className="font-medium">No providers match the selected filters</p>
          </div>
        )}
        {filteredRecords.map((rec) => {
          const isExpanded = expandedId === rec.providerId;
          const isActioning = actionLoading === rec.providerId;
          return (
            <div
              key={rec.providerId}
              className="rounded-xl border border-surface-200 bg-surface-0 transition-all hover:shadow-sm"
            >
              {/* Collapsed row */}
              <button
                type="button"
                onClick={() => setExpandedId(isExpanded ? null : rec.providerId)}
                className="w-full flex items-center justify-between gap-4 p-4 text-left"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <div className={`flex-shrink-0 w-2.5 h-2.5 rounded-full ${rec.provider.status === "online" ? "bg-emerald-500" : rec.provider.status === "degraded" ? "bg-amber-500" : "bg-red-400"}`} />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-surface-900 truncate">{rec.provider.name}</span>
                      <StatusBadge status={rec.status} />
                    </div>
                    <div className="flex items-center gap-3 mt-0.5 text-xs text-surface-800/40">
                      <span>{rec.provider.region}</span>
                      <span>{rec.provider.models.length} models</span>
                      <span>Uptime {rec.provider.uptime.toFixed(1)}%</span>
                      <span>PoNW {rec.ponwScore}</span>
                    </div>
                  </div>
                </div>
                <svg
                  className={`w-4 h-4 text-surface-800/30 flex-shrink-0 transition-transform ${isExpanded ? "rotate-180" : ""}`}
                  viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
                >
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </button>

              {/* Expanded details */}
              {isExpanded && (
                <div className="border-t border-surface-100 px-4 py-4 space-y-4">
                  {/* Verification criteria */}
                  <div>
                    <h4 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wide mb-2">Verification Criteria</h4>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      <CriteriaCheck label={`Uptime > 95% (${rec.provider.uptime.toFixed(1)}%)`} passed={rec.criteria.uptimeOk} />
                      <CriteriaCheck label={`PoNW Score > 800 (${rec.ponwScore})`} passed={rec.criteria.ponwScoreOk} />
                      <CriteriaCheck label={`On-chain box exists${rec.provider.ergoAddress ? ` (${truncateAddr(rec.provider.ergoAddress)})` : ""}`} passed={rec.criteria.onChainBoxExists} />
                      <CriteriaCheck label={`Valid endpoint (${rec.provider.status})`} passed={rec.criteria.validEndpoint} />
                    </div>
                  </div>

                  {/* Provider details */}
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
                    <DetailItem label="GPU" value={rec.provider.gpuInfo} />
                    <DetailItem label="Latency" value={`${rec.provider.latencyMs}ms`} />
                    <DetailItem label="AI Points" value={String(rec.provider.aiPoints)} />
                    <DetailItem label="Price/1M tokens" value={`${(rec.provider.pricePer1mTokens / 1e9).toFixed(2)} ERG`} />
                  </div>

                  {/* Models */}
                  <div>
                    <h4 className="text-xs font-semibold text-surface-800/50 uppercase tracking-wide mb-1.5">Models</h4>
                    <div className="flex flex-wrap gap-1.5">
                      {rec.provider.models.map((m) => (
                        <span key={m} className="px-2 py-0.5 rounded-md bg-surface-100 dark:bg-surface-800 text-xs text-surface-800/60">
                          {m}
                        </span>
                      ))}
                    </div>
                  </div>

                  {/* Last verified */}
                  {rec.lastVerified && (
                    <div className="text-xs text-surface-800/40">
                      Last verified: {new Date(rec.lastVerified).toLocaleString()}
                      {rec.verifiedBy && <span> by {rec.verifiedBy}</span>}
                    </div>
                  )}

                  {/* Admin actions */}
                  <div className="flex items-center gap-2 pt-2 border-t border-surface-100">
                    <button
                      type="button"
                      disabled={isActioning}
                      onClick={() => handleVerify(rec.providerId)}
                      className="px-3 py-1.5 text-xs font-medium rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 transition-colors"
                    >
                      {isActioning && rec.status !== "verified" ? "..." : "Verify"}
                    </button>
                    <button
                      type="button"
                      disabled={isActioning}
                      onClick={() => handleUnverify(rec.providerId)}
                      className="px-3 py-1.5 text-xs font-medium rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-50 transition-colors"
                    >
                      {isActioning && rec.status === "unverified" ? "..." : "Unverify"}
                    </button>
                    <button
                      type="button"
                      disabled={isActioning}
                      onClick={() => handleReverify(rec.providerId)}
                      className="px-3 py-1.5 text-xs font-medium rounded-lg border border-surface-200 bg-surface-0 text-surface-800/70 hover:bg-surface-50 disabled:opacity-50 transition-colors"
                    >
                      Re-verify
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function SummaryCard({ label, value, color, textColor }: { label: string; value: number; color: string; textColor?: string }) {
  return (
    <div className={`rounded-xl ${color} p-4`}>
      <div className={`text-xl font-bold ${textColor || "text-surface-900"}`}>{value}</div>
      <div className={`text-xs ${textColor || "text-surface-800/50"}`}>{label}</div>
    </div>
  );
}

function DetailItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-surface-800/40">{label}</div>
      <div className="font-medium text-surface-900 truncate">{value || "N/A"}</div>
    </div>
  );
}

function buildDetailsText(criteria: VerificationCriteria, ponwScore: number, p: ProviderInfo): string {
  const parts: string[] = [];
  if (!criteria.uptimeOk) parts.push(`uptime ${p.uptime.toFixed(1)}%`);
  if (!criteria.ponwScoreOk) parts.push(`PoNW ${ponwScore}`);
  if (!criteria.onChainBoxExists) parts.push("no on-chain box");
  if (!criteria.validEndpoint) parts.push(`endpoint ${p.status}`);
  if (parts.length === 0) return "All criteria met";
  return `Issues: ${parts.join(", ")}`;
}
