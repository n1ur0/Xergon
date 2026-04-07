"use client";

import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ProposalStatus = "active" | "passed" | "failed" | "closed";

export interface Proposal {
  id: string;
  title: string;
  description: string;
  author: string;
  createdAt: string;
  votingStartsAt: string;
  votingEndsAt: string;
  status: ProposalStatus;
  votesFor: number;
  votesAgainst: number;
  abstain: number;
  quorum: number;
  userVote?: "for" | "against" | "abstain" | null;
  category?: string;
}

interface ProposalCardProps {
  proposal: Proposal;
  onVote?: (proposalId: string, vote: "for" | "against") => void;
  onClick?: (proposalId: string) => void;
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const STATUS_CONFIG: Record<ProposalStatus, { label: string; className: string }> = {
  active: {
    label: "Active",
    className: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  },
  passed: {
    label: "Passed",
    className: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
  },
  failed: {
    label: "Failed",
    className: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  },
  closed: {
    label: "Closed",
    className: "bg-surface-100 text-surface-800/60 dark:bg-surface-800/20 dark:text-surface-800/60",
  },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDeadline(iso: string): string {
  const diff = new Date(iso).getTime() - Date.now();
  if (diff <= 0) return "Ended";
  const days = Math.floor(diff / (1000 * 60 * 60 * 24));
  const hours = Math.floor((diff % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
  if (days > 0) return `${days}d ${hours}h remaining`;
  return `${hours}h remaining`;
}

function truncateAddr(addr: string): string {
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
}

function totalVotes(p: Proposal): number {
  return p.votesFor + p.votesAgainst + p.abstain;
}

function forPercentage(p: Proposal): number {
  const total = totalVotes(p);
  if (total === 0) return 0;
  return Math.round((p.votesFor / total) * 100);
}

function quorumReached(p: Proposal): boolean {
  return totalVotes(p) >= p.quorum;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProposalCard({ proposal, onVote, onClick }: ProposalCardProps) {
  const config = STATUS_CONFIG[proposal.status];
  const forPct = forPercentage(proposal);
  const againstPct = 100 - forPct;
  const total = totalVotes(proposal);
  const isActive = proposal.status === "active";

  return (
    <div
      className={cn(
        "group rounded-xl border border-surface-200 bg-surface-0 transition-all hover:shadow-sm hover:border-surface-300",
        onClick && "cursor-pointer",
      )}
      onClick={() => onClick?.(proposal.id)}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={(e) => {
        if (onClick && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onClick(proposal.id);
        }
      }}
    >
      <div className="p-4">
        {/* Header: category + status + deadline */}
        <div className="flex items-center gap-2 mb-2 flex-wrap">
          {proposal.category && (
            <span className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-[10px] font-medium text-surface-800/50">
              {proposal.category}
            </span>
          )}
          <span
            className={cn(
              "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium",
              config.className,
            )}
          >
            {isActive && (
              <span className="mr-1.5 h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
            )}
            {config.label}
          </span>
          {isActive && (
            <span className="text-[10px] text-surface-800/30">
              {formatDeadline(proposal.votingEndsAt)}
            </span>
          )}
        </div>

        {/* Title */}
        <h3 className="text-base font-semibold text-surface-900 group-hover:text-brand-600 transition-colors leading-tight mb-1">
          {proposal.title}
        </h3>

        {/* Description preview */}
        <p className="text-sm text-surface-800/50 line-clamp-2 mb-3">
          {proposal.description}
        </p>

        {/* Author + date */}
        <div className="flex items-center gap-3 text-[10px] text-surface-800/30 mb-4">
          <span>by {truncateAddr(proposal.author)}</span>
          <span>{new Date(proposal.createdAt).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" })}</span>
        </div>

        {/* Vote progress bar */}
        <div className="mb-3">
          <div className="flex items-center justify-between mb-1">
            <span className="text-xs font-medium text-emerald-600 dark:text-emerald-400">
              For {proposal.votesFor.toLocaleString()}
            </span>
            <span className="text-xs font-medium text-red-600 dark:text-red-400">
              Against {proposal.votesAgainst.toLocaleString()}
            </span>
          </div>
          <div className="h-2 rounded-full bg-surface-100 overflow-hidden flex">
            <div
              className="h-full bg-emerald-500 transition-all rounded-l-full"
              style={{ width: `${forPct}%` }}
            />
            <div
              className="h-full bg-red-400 transition-all rounded-r-full"
              style={{ width: `${againstPct}%` }}
            />
          </div>
          <div className="flex items-center justify-between mt-1">
            <span className="text-[10px] text-surface-800/30">{forPct}% for</span>
            <span className="text-[10px] text-surface-800/30">
              {total.toLocaleString()} votes &middot; {quorumReached(proposal) ? "Quorum reached" : `${Math.round(((total / proposal.quorum) * 100))}% quorum`}
            </span>
          </div>
        </div>

        {/* Vote buttons */}
        {isActive && onVote && (
          <div className="flex items-center gap-2">
            <button
              onClick={(e) => {
                e.stopPropagation();
                onVote(proposal.id, "for");
              }}
              disabled={proposal.userVote === "for"}
              className={cn(
                "flex-1 rounded-lg border px-3 py-2 text-xs font-semibold transition-colors",
                proposal.userVote === "for"
                  ? "bg-emerald-50 border-emerald-300 text-emerald-700 dark:bg-emerald-950/30 dark:border-emerald-700 dark:text-emerald-400"
                  : "border-surface-200 bg-surface-50 text-surface-800/60 hover:bg-emerald-50 hover:text-emerald-700 hover:border-emerald-200",
              )}
            >
              Vote For
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onVote(proposal.id, "against");
              }}
              disabled={proposal.userVote === "against"}
              className={cn(
                "flex-1 rounded-lg border px-3 py-2 text-xs font-semibold transition-colors",
                proposal.userVote === "against"
                  ? "bg-red-50 border-red-300 text-red-700 dark:bg-red-950/30 dark:border-red-700 dark:text-red-400"
                  : "border-surface-200 bg-surface-50 text-surface-800/60 hover:bg-red-50 hover:text-red-700 hover:border-red-200",
              )}
            >
              Vote Against
            </button>
            {proposal.userVote && (
              <span className="text-[10px] text-surface-800/30 italic">
                You voted {proposal.userVote}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
