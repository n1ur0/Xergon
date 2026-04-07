"use client";

import { useState, useEffect, useMemo, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ContentType = "review" | "forum_post" | "model_listing";
type ContentStatus = "pending" | "approved" | "dismissed" | "deleted";
type SortOption = "newest" | "most_flags" | "unresolved";

interface FlaggedContent {
  id: string;
  type: ContentType;
  title: string;
  author: string;
  authorPk: string;
  flagCount: number;
  reason: string;
  status: ContentStatus;
  flaggedAt: string;
  resolvedAt?: string;
  resolvedBy?: string;
  contentPreview: string;
  autoFlagged: boolean;
  autoFlagReason?: string;
  suspiciousPatterns: string[];
}

interface ModerationStats {
  total: number;
  pending: number;
  approved: number;
  dismissed: number;
  deleted: number;
  autoFlagged: number;
  averageFlagCount: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const TYPE_LABELS: Record<ContentType, string> = {
  review: "Review",
  forum_post: "Forum Post",
  model_listing: "Model Listing",
};

const TYPE_COLORS: Record<ContentType, string> = {
  review: "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
  forum_post: "bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300",
  model_listing: "bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-300",
};

const STATUS_COLORS: Record<ContentStatus, string> = {
  pending: "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300",
  approved: "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300",
  dismissed: "bg-surface-100 text-surface-800/60 dark:bg-surface-800 dark:text-surface-400",
  deleted: "bg-gray-100 text-gray-800 dark:bg-gray-900/30 dark:text-gray-300",
};

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ReviewModerationPanel() {
  const [items, setItems] = useState<FlaggedContent[]>([]);
  const [stats, setStats] = useState<ModerationStats | null>(null);
  const [typeFilter, setTypeFilter] = useState<ContentType | "all">("all");
  const [statusFilter, setStatusFilter] = useState<ContentStatus | "all">("all");
  const [sort, setSort] = useState<SortOption>("newest");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  // Fetch flagged content
  const fetchData = useCallback(async () => {
    try {
      const params = new URLSearchParams();
      if (typeFilter !== "all") params.set("type", typeFilter);
      if (statusFilter !== "all") params.set("status", statusFilter);
      params.set("sort", sort);

      const res = await fetch(`/api/admin/moderation?${params}`);
      if (res.ok) {
        const data = await res.json();
        setItems(data.items ?? []);
        setStats(data.stats ?? null);
      }
    } catch {
      // silently fail
    } finally {
      setLoading(false);
    }
  }, [typeFilter, statusFilter, sort]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const filtered = useMemo(() => {
    let list = [...items];
    if (typeFilter !== "all") list = list.filter((f) => f.type === typeFilter);
    if (statusFilter !== "all") list = list.filter((f) => f.status === statusFilter);

    switch (sort) {
      case "newest":
        list.sort((a, b) => new Date(b.flaggedAt).getTime() - new Date(a.flaggedAt).getTime());
        break;
      case "most_flags":
        list.sort((a, b) => b.flagCount - a.flagCount);
        break;
      case "unresolved":
        list.sort((a, b) => {
          if (a.status === "pending" && b.status !== "pending") return -1;
          if (a.status !== "pending" && b.status === "pending") return 1;
          return new Date(b.flaggedAt).getTime() - new Date(a.flaggedAt).getTime();
        });
        break;
    }
    return list;
  }, [items, typeFilter, statusFilter, sort]);

  // Actions via API
  const performAction = async (action: string, ids: string[]) => {
    setActionLoading(action);
    try {
      const res = await fetch("/api/admin/moderation", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action, ids }),
      });
      if (res.ok) {
        await fetchData();
        setSelectedIds(new Set());
      }
    } catch {
      // silently fail
    } finally {
      setActionLoading(null);
    }
  };

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    const pendingIds = filtered.filter((f) => f.status === "pending").map((f) => f.id);
    if (pendingIds.every((id) => selectedIds.has(id))) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(pendingIds));
    }
  };

  const isActionLoading = actionLoading !== null;

  return (
    <div className="space-y-6">
      {/* Stats dashboard */}
      {stats && (
        <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-7 gap-3">
          <StatCard label="Total" value={stats.total} color="bg-surface-100 dark:bg-surface-800" />
          <StatCard label="Pending" value={stats.pending} color="bg-red-50 dark:bg-red-900/10 text-red-700 dark:text-red-300" />
          <StatCard label="Approved" value={stats.approved} color="bg-green-50 dark:bg-green-900/10 text-green-700 dark:text-green-300" />
          <StatCard label="Dismissed" value={stats.dismissed} color="bg-surface-50 dark:bg-surface-800" />
          <StatCard label="Deleted" value={stats.deleted} color="bg-gray-50 dark:bg-gray-900/10 text-gray-700 dark:text-gray-300" />
          <StatCard label="Auto-flagged" value={stats.autoFlagged} color="bg-amber-50 dark:bg-amber-900/10 text-amber-700 dark:text-amber-300" />
          <StatCard label="Avg Flags" value={stats.averageFlagCount.toFixed(1)} color="bg-surface-50 dark:bg-surface-800" />
        </div>
      )}

      {/* Header + Filters */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-semibold text-surface-900 dark:text-surface-100">Review Moderation</h2>
          {stats && (
            <p className="text-sm text-surface-800/50">{stats.pending} pending review</p>
          )}
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <select
            value={typeFilter}
            onChange={(e) => setTypeFilter(e.target.value as ContentType | "all")}
            className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 text-surface-900 dark:text-surface-100"
          >
            <option value="all">All Types</option>
            <option value="review">Reviews</option>
            <option value="forum_post">Forum Posts</option>
            <option value="model_listing">Model Listings</option>
          </select>
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as ContentStatus | "all")}
            className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 text-surface-900 dark:text-surface-100"
          >
            <option value="all">All Statuses</option>
            <option value="pending">Pending</option>
            <option value="approved">Approved</option>
            <option value="dismissed">Dismissed</option>
            <option value="deleted">Deleted</option>
          </select>
          <select
            value={sort}
            onChange={(e) => setSort(e.target.value as SortOption)}
            className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 text-surface-900 dark:text-surface-100"
          >
            <option value="newest">Newest First</option>
            <option value="most_flags">Most Flags</option>
            <option value="unresolved">Unresolved First</option>
          </select>
        </div>
      </div>

      {/* Bulk actions */}
      {selectedIds.size > 0 && (
        <div className="flex items-center gap-3 rounded-lg border border-brand-200 bg-brand-50/50 dark:border-brand-800 dark:bg-brand-900/10 px-4 py-2.5">
          <span className="text-sm font-medium text-surface-900 dark:text-surface-100">
            {selectedIds.size} selected
          </span>
          <button
            onClick={() => performAction("bulk_approve", Array.from(selectedIds))}
            disabled={isActionLoading}
            className="px-3 py-1 text-xs font-medium rounded-md bg-green-600 text-white hover:bg-green-700 transition-colors disabled:opacity-50"
          >
            Approve All
          </button>
          <button
            onClick={() => performAction("bulk_dismiss", Array.from(selectedIds))}
            disabled={isActionLoading}
            className="px-3 py-1 text-xs font-medium rounded-md bg-surface-200 text-surface-800 dark:bg-surface-700 dark:text-surface-200 hover:bg-surface-300 transition-colors disabled:opacity-50"
          >
            Dismiss All
          </button>
          <button
            onClick={() => performAction("bulk_delete", Array.from(selectedIds))}
            disabled={isActionLoading}
            className="px-3 py-1 text-xs font-medium rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors disabled:opacity-50"
          >
            Delete All
          </button>
          <button
            onClick={() => setSelectedIds(new Set())}
            className="text-xs text-surface-800/50 hover:text-surface-900 ml-auto"
          >
            Clear selection
          </button>
        </div>
      )}

      {/* Content list */}
      <div className="space-y-2">
        {loading ? (
          <div className="space-y-3 animate-pulse">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="rounded-xl border border-surface-200 dark:border-surface-700 p-4">
                <div className="h-4 w-48 bg-surface-200 dark:bg-surface-700 rounded mb-2" />
                <div className="h-3 w-64 bg-surface-100 dark:bg-surface-800 rounded" />
              </div>
            ))}
          </div>
        ) : (
          filtered.map((flag) => (
            <div
              key={flag.id}
              className={`rounded-xl border bg-surface-0 dark:bg-surface-900 transition-colors ${
                flag.status === "pending"
                  ? "border-surface-200 dark:border-surface-700"
                  : "border-surface-100 dark:border-surface-800 opacity-70"
              }`}
            >
              <div className="flex items-start gap-3 p-4">
                {/* Checkbox */}
                {flag.status === "pending" && (
                  <input
                    type="checkbox"
                    checked={selectedIds.has(flag.id)}
                    onChange={() => toggleSelect(flag.id)}
                    className="mt-1 h-4 w-4 rounded border-surface-300 text-brand-600 focus:ring-brand-500"
                  />
                )}

                {/* Content info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 flex-wrap mb-1">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${TYPE_COLORS[flag.type]}`}>
                      {TYPE_LABELS[flag.type]}
                    </span>
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${STATUS_COLORS[flag.status]}`}>
                      {flag.status}
                    </span>
                    {flag.autoFlagged && (
                      <span className="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
                        Auto-flagged
                      </span>
                    )}
                    {flag.flagCount > 1 && (
                      <span className="text-xs text-surface-800/40">
                        {flag.flagCount} flags
                      </span>
                    )}
                  </div>
                  <p className="text-sm font-medium text-surface-900 dark:text-surface-100 mb-0.5">
                    {flag.title}
                  </p>
                  <p className="text-xs text-surface-800/50">
                    by <span className="font-mono">{flag.author}</span> · {timeAgo(flag.flaggedAt)}
                  </p>
                  <p className="text-xs text-surface-800/40 mt-1">
                    Reason: {flag.reason}
                  </p>

                  {/* Suspicious patterns */}
                  {flag.suspiciousPatterns.length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-2">
                      {flag.suspiciousPatterns.map((pattern) => (
                        <span
                          key={pattern}
                          className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-red-50 text-red-600 dark:bg-red-900/20 dark:text-red-400"
                        >
                          {pattern}
                        </span>
                      ))}
                    </div>
                  )}

                  {/* Expandable preview */}
                  {expandedId === flag.id && (
                    <div className="mt-3 rounded-lg bg-surface-100 dark:bg-surface-800 p-3 text-sm text-surface-800/70 dark:text-surface-300">
                      {flag.contentPreview}
                    </div>
                  )}

                  {/* Resolution info */}
                  {flag.resolvedAt && (
                    <p className="text-[10px] text-surface-800/30 mt-1">
                      Resolved {timeAgo(flag.resolvedAt)}{flag.resolvedBy ? ` by ${flag.resolvedBy}` : ""}
                    </p>
                  )}
                </div>

                {/* Actions */}
                {flag.status === "pending" ? (
                  <div className="flex items-center gap-1.5 shrink-0">
                    <button
                      onClick={() => setExpandedId(expandedId === flag.id ? null : flag.id)}
                      className="px-2.5 py-1 text-xs font-medium rounded-md text-surface-800/60 hover:bg-surface-100 dark:hover:bg-surface-800 transition-colors"
                      title="Preview content"
                    >
                      Preview
                    </button>
                    <button
                      onClick={() => performAction("bulk_approve", [flag.id])}
                      disabled={isActionLoading}
                      className="px-2.5 py-1 text-xs font-medium rounded-md bg-green-600 text-white hover:bg-green-700 transition-colors disabled:opacity-50"
                    >
                      Approve
                    </button>
                    <button
                      onClick={() => performAction("bulk_dismiss", [flag.id])}
                      disabled={isActionLoading}
                      className="px-2.5 py-1 text-xs font-medium rounded-md bg-surface-200 text-surface-800 dark:bg-surface-700 dark:text-surface-200 hover:bg-surface-300 transition-colors disabled:opacity-50"
                    >
                      Dismiss
                    </button>
                    <button
                      onClick={() => performAction("bulk_delete", [flag.id])}
                      disabled={isActionLoading}
                      className="px-2.5 py-1 text-xs font-medium rounded-md text-red-600 border border-red-200 dark:border-red-800 hover:bg-red-50 dark:hover:bg-red-900/10 transition-colors disabled:opacity-50"
                    >
                      Delete
                    </button>
                  </div>
                ) : (
                  <span className="text-xs text-surface-800/30 shrink-0 italic">
                    Resolved
                  </span>
                )}
              </div>
            </div>
          ))
        )}

        {!loading && filtered.length === 0 && (
          <div className="text-center py-12 text-surface-800/40">
            No flagged content matches the current filters.
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function StatCard({ label, value, color }: { label: string; value: string | number; color: string }) {
  return (
    <div className={`rounded-lg border border-surface-200 dark:border-surface-700 p-3 ${color}`}>
      <div className="text-lg font-bold text-surface-900 dark:text-surface-100">{value}</div>
      <div className="text-xs text-surface-800/50">{label}</div>
    </div>
  );
}
