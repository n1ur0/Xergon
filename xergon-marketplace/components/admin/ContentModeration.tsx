"use client";

import { useState, useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ContentType = "review" | "forum_post" | "model_listing";
type ContentStatus = "pending" | "approved" | "dismissed";
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
  contentPreview: string;
}

// ---------------------------------------------------------------------------
// Mock data (in production this would come from /api/admin/moderation)
// ---------------------------------------------------------------------------

const MOCK_FLAGS: FlaggedContent[] = [
  {
    id: "flag-1",
    type: "review",
    title: "Inappropriate language in review",
    author: "0xabc...123",
    authorPk: "abc123",
    flagCount: 3,
    reason: "Contains offensive language",
    status: "pending",
    flaggedAt: "2026-04-05T10:00:00Z",
    contentPreview: "This provider is terrible and uses bad words...",
  },
  {
    id: "flag-2",
    type: "forum_post",
    title: "Spam post in General Discussion",
    author: "0xdef...456",
    authorPk: "def456",
    flagCount: 5,
    reason: "Spam / self-promotion",
    status: "pending",
    flaggedAt: "2026-04-04T15:30:00Z",
    contentPreview: "Check out my amazing service at external-link.com...",
  },
  {
    id: "flag-3",
    type: "model_listing",
    title: "Misleading model description",
    author: "0x789...abc",
    authorPk: "789abc",
    flagCount: 2,
    reason: "Model claims capabilities it does not have",
    status: "pending",
    flaggedAt: "2026-04-03T08:00:00Z",
    contentPreview: "This model achieves 99% accuracy on all benchmarks...",
  },
  {
    id: "flag-4",
    type: "review",
    title: "Fake review detected",
    author: "0x111...222",
    authorPk: "111222",
    flagCount: 7,
    reason: "Reviewer has no rental history with this provider",
    status: "pending",
    flaggedAt: "2026-04-02T12:00:00Z",
    contentPreview: "Best provider ever! 10/10 would rent again!",
  },
  {
    id: "flag-5",
    type: "forum_post",
    title: "Off-topic discussion",
    author: "0x333...444",
    authorPk: "333444",
    flagCount: 1,
    reason: "Completely unrelated to Xergon marketplace",
    status: "approved",
    flaggedAt: "2026-04-01T20:00:00Z",
    contentPreview: "What do you think about the weather today?",
  },
];

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

export function ContentModeration() {
  const [flags, setFlags] = useState<FlaggedContent[]>(MOCK_FLAGS);
  const [typeFilter, setTypeFilter] = useState<ContentType | "all">("all");
  const [sort, setSort] = useState<SortOption>("newest");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    let list = [...flags];
    if (typeFilter !== "all") list = list.filter((f) => f.type === typeFilter);

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
  }, [flags, typeFilter, sort]);

  const pendingCount = flags.filter((f) => f.status === "pending").length;

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

  const updateFlag = (id: string, updates: Partial<FlaggedContent>) => {
    setFlags((prev) => prev.map((f) => (f.id === id ? { ...f, ...updates } : f)));
    setSelectedIds((prev) => { const next = new Set(prev); next.delete(id); return next; });
  };

  const handleApprove = (id: string) => updateFlag(id, { status: "approved" });
  const handleDismiss = (id: string) => updateFlag(id, { status: "dismissed" });
  const handleBanUser = (authorPk: string) => {
    setFlags((prev) => prev.map((f) => (f.authorPk === authorPk ? { ...f, status: "dismissed" as ContentStatus } : f)));
    setSelectedIds(new Set());
  };
  const handleDelete = (id: string) => updateFlag(id, { status: "dismissed" });

  const handleBulkApprove = () => {
    setFlags((prev) => prev.map((f) => (selectedIds.has(f.id) ? { ...f, status: "approved" as ContentStatus } : f)));
    setSelectedIds(new Set());
  };
  const handleBulkDismiss = () => {
    setFlags((prev) => prev.map((f) => (selectedIds.has(f.id) ? { ...f, status: "dismissed" as ContentStatus } : f)));
    setSelectedIds(new Set());
  };

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="text-lg font-semibold text-surface-900">Content Moderation</h2>
          <p className="text-sm text-surface-800/50">{pendingCount} pending review</p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <select
            value={typeFilter}
            onChange={(e) => setTypeFilter(e.target.value as ContentType | "all")}
            className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
          >
            <option value="all">All Types</option>
            <option value="review">Reviews</option>
            <option value="forum_post">Forum Posts</option>
            <option value="model_listing">Model Listings</option>
          </select>
          <select
            value={sort}
            onChange={(e) => setSort(e.target.value as SortOption)}
            className="px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
          >
            <option value="newest">Newest First</option>
            <option value="most_flags">Most Flags</option>
            <option value="unresolved">Unresolved First</option>
          </select>
        </div>
      </div>

      {/* Bulk actions */}
      {selectedIds.size > 0 && (
        <div className="flex items-center gap-3 rounded-lg border border-brand-200 bg-brand-50/50 px-4 py-2.5 dark:border-brand-800 dark:bg-brand-900/10">
          <span className="text-sm font-medium text-surface-900">
            {selectedIds.size} selected
          </span>
          <button
            onClick={handleBulkApprove}
            className="px-3 py-1 text-xs font-medium rounded-md bg-green-600 text-white hover:bg-green-700 transition-colors"
          >
            Approve All
          </button>
          <button
            onClick={handleBulkDismiss}
            className="px-3 py-1 text-xs font-medium rounded-md bg-surface-200 text-surface-800 hover:bg-surface-300 transition-colors dark:bg-surface-700 dark:text-surface-200"
          >
            Dismiss All
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
        {filtered.map((flag) => (
          <div
            key={flag.id}
            className={`rounded-xl border bg-surface-0 transition-colors ${
              flag.status === "pending"
                ? "border-surface-200"
                : "border-surface-100 opacity-70"
            }`}
          >
            <div className="flex items-start gap-3 p-4">
              {/* Checkbox (only for pending) */}
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
                  {flag.flagCount > 1 && (
                    <span className="text-xs text-surface-800/40">
                      {flag.flagCount} flags
                    </span>
                  )}
                </div>
                <p className="text-sm font-medium text-surface-900 mb-0.5">
                  {flag.title}
                </p>
                <p className="text-xs text-surface-800/50">
                  by <span className="font-mono">{flag.author}</span> · {timeAgo(flag.flaggedAt)}
                </p>
                <p className="text-xs text-surface-800/40 mt-1">
                  Reason: {flag.reason}
                </p>

                {/* Expandable preview */}
                {expandedId === flag.id && (
                  <div className="mt-3 rounded-lg bg-surface-100 p-3 text-sm text-surface-800/70 dark:bg-surface-800 dark:text-surface-300">
                    {flag.contentPreview}
                  </div>
                )}
              </div>

              {/* Actions */}
              {flag.status === "pending" ? (
                <div className="flex items-center gap-1.5 shrink-0">
                  <button
                    onClick={() => setExpandedId(expandedId === flag.id ? null : flag.id)}
                    className="px-2.5 py-1 text-xs font-medium rounded-md text-surface-800/60 hover:bg-surface-100 transition-colors dark:hover:bg-surface-800"
                    title="Preview content"
                  >
                    Preview
                  </button>
                  <button
                    onClick={() => handleApprove(flag.id)}
                    className="px-2.5 py-1 text-xs font-medium rounded-md bg-green-600 text-white hover:bg-green-700 transition-colors"
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => handleDismiss(flag.id)}
                    className="px-2.5 py-1 text-xs font-medium rounded-md bg-surface-200 text-surface-800 hover:bg-surface-300 transition-colors dark:bg-surface-700 dark:text-surface-200"
                  >
                    Dismiss
                  </button>
                  <button
                    onClick={() => handleBanUser(flag.authorPk)}
                    className="px-2.5 py-1 text-xs font-medium rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors"
                    title="Ban user and dismiss all their flags"
                  >
                    Ban User
                  </button>
                  <button
                    onClick={() => handleDelete(flag.id)}
                    className="px-2.5 py-1 text-xs font-medium rounded-md text-red-600 border border-red-200 hover:bg-red-50 transition-colors dark:border-red-800 dark:hover:bg-red-900/10"
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
        ))}

        {filtered.length === 0 && (
          <div className="text-center py-12 text-surface-800/40">
            No flagged content matches the current filters.
          </div>
        )}
      </div>
    </div>
  );
}
