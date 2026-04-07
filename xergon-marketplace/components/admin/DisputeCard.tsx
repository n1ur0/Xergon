"use client";

import { useState } from "react";
import type { Dispute } from "@/app/api/admin/disputes/route";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(diff / 60_000);
  const hours = Math.floor(diff / 3_600_000);
  const days = Math.floor(diff / 86_400_000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return `${days}d ago`;
}

function truncatePk(pk: string): string {
  if (pk.length <= 14) return pk;
  return `${pk.slice(0, 10)}...${pk.slice(-4)}`;
}

// ---------------------------------------------------------------------------
// Status / type badges
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: Dispute["status"] }) {
  const colors: Record<string, string> = {
    open: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
    investigating: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    resolved: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    dismissed: "bg-surface-100 text-surface-600 dark:bg-surface-800 dark:text-surface-400",
  };
  return (
    <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${colors[status] ?? "bg-surface-100"}`}>
      {status}
    </span>
  );
}

function TypeBadge({ type }: { type: Dispute["type"] }) {
  const colors: Record<string, string> = {
    quality: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
    downtime: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    payment: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    fraud: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  };
  const icons: Record<string, string> = {
    quality: "Q",
    downtime: "D",
    payment: "P",
    fraud: "F",
  };
  return (
    <span className={`inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-xs font-medium ${colors[type] ?? "bg-surface-100"}`}>
      <span className="w-4 h-4 rounded-full bg-current/10 flex items-center justify-center text-[10px] font-bold">
        {icons[type]}
      </span>
      {type}
    </span>
  );
}

// ---------------------------------------------------------------------------
// DisputeCard
// ---------------------------------------------------------------------------

const RESOLUTION_ACTIONS = [
  { value: "dismiss", label: "Dismiss", color: "text-surface-600" },
  { value: "warn_provider", label: "Warn Provider", color: "text-amber-600" },
  { value: "slash_provider", label: "Slash Provider", color: "text-orange-600" },
  { value: "suspend_provider", label: "Suspend Provider", color: "text-red-600" },
] as const;

export function DisputeCard({
  dispute,
  onResolve,
}: {
  dispute: Dispute;
  onResolve: (id: string, action: string, resolution: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [resolving, setResolving] = useState(false);
  const [action, setAction] = useState("dismiss");
  const [resolution, setResolution] = useState("");

  const isResolvable = dispute.status === "open" || dispute.status === "investigating";

  const handleResolve = async () => {
    if (!resolution.trim() || resolution.length < 5) return;
    setResolving(true);
    try {
      const res = await fetch(`/api/admin/disputes/${encodeURIComponent(dispute.id)}/resolve`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ resolution, action }),
      });
      if (res.ok) {
        onResolve(dispute.id, action, resolution);
        setResolving(false);
        setExpanded(false);
      }
    } catch {
      setResolving(false);
    }
  };

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden transition-all hover:shadow-sm">
      {/* Header */}
      <div className="px-5 py-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-surface-900">{dispute.id}</span>
            <StatusBadge status={dispute.status} />
            <TypeBadge type={dispute.type} />
          </div>
          <div className="text-xs text-surface-800/40 whitespace-nowrap">
            {relativeTime(dispute.createdAt)}
          </div>
        </div>

        <p className="mt-2 text-sm text-surface-800/70 leading-relaxed">
          {dispute.description}
        </p>

        <div className="mt-2 flex items-center gap-4 text-xs text-surface-800/40">
          <span>
            Reporter: <span className="font-mono">{truncatePk(dispute.reporterAddress)}</span>
          </span>
          <span>
            Provider: <span className="font-mono">{truncatePk(dispute.providerPk)}</span>
          </span>
          <span>{dispute.evidence.length} evidence items</span>
        </div>

        {/* Actions row */}
        <div className="mt-3 flex items-center gap-2">
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            className="text-xs font-medium text-brand-600 hover:text-brand-700 transition-colors"
          >
            {expanded ? "Hide details" : "Show details"}
          </button>

          {isResolvable && !expanded && (
            <button
              type="button"
              onClick={() => {
                setExpanded(true);
                setResolving(true);
              }}
              className="text-xs font-medium px-3 py-1.5 rounded-lg bg-brand-50 text-brand-600 hover:bg-brand-100 dark:bg-brand-950/20 dark:hover:bg-brand-950/30 transition-colors"
            >
              Resolve
            </button>
          )}

          {dispute.resolution && (
            <div className="text-xs text-surface-800/50 italic">
              Resolution: {dispute.resolution}
            </div>
          )}
        </div>
      </div>

      {/* Expanded details */}
      {expanded && (
        <div className="border-t border-surface-100 bg-surface-50 px-5 py-4 space-y-4">
          {/* Evidence */}
          {dispute.evidence.length > 0 && (
            <div>
              <h4 className="text-xs font-medium uppercase tracking-wider text-surface-800/50 mb-2">
                Evidence ({dispute.evidence.length})
              </h4>
              <ul className="space-y-1.5">
                {dispute.evidence.map((ev, i) => (
                  <li key={i} className="flex items-start gap-2 text-sm text-surface-800/60">
                    <svg className="w-4 h-4 text-surface-800/30 flex-shrink-0 mt-0.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                      <polyline points="14 2 14 8 20 8" />
                    </svg>
                    <span>{ev}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Resolution form */}
          {isResolvable && (
            <div className="space-y-3 pt-2 border-t border-surface-200">
              <h4 className="text-xs font-medium uppercase tracking-wider text-surface-800/50">
                Resolve Dispute
              </h4>

              <div>
                <label htmlFor={`action-${dispute.id}`} className="block text-xs font-medium text-surface-800/60 mb-1">
                  Action
                </label>
                <select
                  id={`action-${dispute.id}`}
                  value={action}
                  onChange={(e) => setAction(e.target.value)}
                  className="w-full max-w-xs px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
                >
                  {RESOLUTION_ACTIONS.map((a) => (
                    <option key={a.value} value={a.value}>{a.label}</option>
                  ))}
                </select>
              </div>

              <div>
                <label htmlFor={`resolution-${dispute.id}`} className="block text-xs font-medium text-surface-800/60 mb-1">
                  Resolution notes
                </label>
                <textarea
                  id={`resolution-${dispute.id}`}
                  value={resolution}
                  onChange={(e) => setResolution(e.target.value)}
                  placeholder="Describe the resolution..."
                  rows={3}
                  className="w-full px-3 py-2 text-sm rounded-lg border border-surface-200 bg-surface-0 placeholder-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 resize-none"
                />
              </div>

              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleResolve}
                  disabled={resolving || resolution.length < 5}
                  className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {resolving ? (
                    <>
                      <span className="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2" />
                      Submitting...
                    </>
                  ) : (
                    "Submit Resolution"
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setExpanded(false);
                    setResolving(false);
                    setResolution("");
                  }}
                  className="inline-flex items-center px-4 py-2 rounded-lg text-sm font-medium text-surface-800/50 hover:text-surface-800/70 hover:bg-surface-100 transition-colors"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          {/* Timestamps */}
          <div className="flex gap-6 text-xs text-surface-800/30">
            <span>Created: {new Date(dispute.createdAt).toLocaleString()}</span>
            <span>Updated: {new Date(dispute.updatedAt).toLocaleString()}</span>
            {dispute.resolvedAt && (
              <span>Resolved: {new Date(dispute.resolvedAt).toLocaleString()}</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
