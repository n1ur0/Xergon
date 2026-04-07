"use client";

import { useState, useEffect, useCallback } from "react";
import { cn } from "@/lib/utils";
import { ProposalCard, type Proposal, type ProposalStatus } from "@/components/governance/ProposalCard";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface VotingPower {
  ergBalance: number;
  xrgStaked: number;
  votingPower: number;
  delegations: number;
}

interface GovernanceData {
  proposals: Proposal[];
  votingPower: VotingPower;
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

function formatErg(amount: number): string {
  return amount.toFixed(4);
}

function truncateAddr(addr: string): string {
  if (addr.length <= 14) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-4)}`;
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

// ---------------------------------------------------------------------------
// Create Proposal Form
// ---------------------------------------------------------------------------

function CreateProposalForm({
  onSubmit,
  isSubmitting,
}: {
  onSubmit: (data: { title: string; description: string; category: string }) => void;
  isSubmitting: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [category, setCategory] = useState("general");

  const categories = [
    { value: "general", label: "General" },
    { value: "protocol", label: "Protocol" },
    { value: "treasury", label: "Treasury" },
    { value: "model-listing", label: "Model Listing" },
    { value: "provider", label: "Provider" },
  ];

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !description.trim()) return;
    onSubmit({ title: title.trim(), description: description.trim(), category });
    setTitle("");
    setDescription("");
    setCategory("general");
    setIsOpen(false);
  };

  if (!isOpen) {
    return (
      <button
        onClick={() => setIsOpen(true)}
        className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-brand-700"
      >
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
        </svg>
        Create Proposal
      </button>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm space-y-4">
      <h3 className="text-base font-semibold text-surface-900">Create New Proposal</h3>

      <div>
        <label className="block text-xs font-medium text-surface-800/60 mb-1">Category</label>
        <select
          value={category}
          onChange={(e) => setCategory(e.target.value)}
          className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm text-surface-900"
        >
          {categories.map((c) => (
            <option key={c.value} value={c.value}>{c.label}</option>
          ))}
        </select>
      </div>

      <div>
        <label className="block text-xs font-medium text-surface-800/60 mb-1">Title</label>
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Brief title for your proposal"
          className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm text-surface-900 placeholder:text-surface-800/30"
          required
        />
      </div>

      <div>
        <label className="block text-xs font-medium text-surface-800/60 mb-1">Description</label>
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Describe your proposal in detail..."
          rows={4}
          className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm text-surface-900 placeholder:text-surface-800/30 resize-none"
          required
        />
      </div>

      <div className="flex items-center gap-2 justify-end">
        <button
          type="button"
          onClick={() => setIsOpen(false)}
          className="rounded-lg border border-surface-200 bg-surface-50 px-4 py-2 text-xs font-medium text-surface-800/60 transition-colors hover:bg-surface-100"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={isSubmitting || !title.trim() || !description.trim()}
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-xs font-semibold text-white transition-colors hover:bg-brand-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {isSubmitting ? "Submitting..." : "Submit Proposal"}
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Proposal Detail Modal
// ---------------------------------------------------------------------------

function ProposalDetailModal({
  proposal,
  onClose,
}: {
  proposal: Proposal;
  onClose: () => void;
}) {
  const totalVotes = proposal.votesFor + proposal.votesAgainst + proposal.abstain;
  const forPct = totalVotes > 0 ? Math.round((proposal.votesFor / totalVotes) * 100) : 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" />
      <div
        className="relative w-full max-w-lg rounded-2xl border border-surface-200 bg-surface-0 p-6 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 text-surface-800/30 hover:text-surface-800/60 transition-colors"
        >
          <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>

        <div className="flex items-center gap-2 mb-3">
          <span className={cn(
            "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium capitalize",
            proposal.status === "active" ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400" :
            proposal.status === "passed" ? "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400" :
            proposal.status === "failed" ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400" :
            "bg-surface-100 text-surface-800/60",
          )}>
            {proposal.status}
          </span>
          {proposal.category && (
            <span className="inline-flex items-center rounded-full bg-surface-100 px-2 py-0.5 text-[10px] font-medium text-surface-800/50 capitalize">
              {proposal.category}
            </span>
          )}
        </div>

        <h2 className="text-xl font-bold text-surface-900 mb-2">{proposal.title}</h2>

        <div className="text-xs text-surface-800/40 mb-4 space-y-1">
          <div>by {truncateAddr(proposal.author)}</div>
          <div>Created: {new Date(proposal.createdAt).toLocaleDateString()}</div>
          <div>Voting: {new Date(proposal.votingStartsAt).toLocaleDateString()} - {new Date(proposal.votingEndsAt).toLocaleDateString()}</div>
        </div>

        <div className="text-sm text-surface-800/60 leading-relaxed mb-6 whitespace-pre-wrap">
          {proposal.description}
        </div>

        {/* Voting results */}
        <div className="rounded-lg border border-surface-200 p-4">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Voting Results</h3>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs text-emerald-600 dark:text-emerald-400 font-medium">For: {proposal.votesFor.toLocaleString()}</span>
              <span className="text-xs text-surface-800/40">{forPct}%</span>
            </div>
            <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
              <div className="h-full rounded-full bg-emerald-500" style={{ width: `${forPct}%` }} />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-xs text-red-600 dark:text-red-400 font-medium">Against: {proposal.votesAgainst.toLocaleString()}</span>
              <span className="text-xs text-surface-800/40">{100 - forPct}%</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-xs text-surface-800/50">Abstain: {proposal.abstain.toLocaleString()}</span>
              <span className="text-xs text-surface-800/40">Total: {totalVotes.toLocaleString()}</span>
            </div>
            <div className="text-[10px] text-surface-800/30 mt-1">
              Quorum: {proposal.quorum.toLocaleString()} ({totalVotes >= proposal.quorum ? "reached" : "not reached"})
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function GovernanceDashboard() {
  const [data, setData] = useState<GovernanceData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<"active" | "past" | "all">("active");
  const [selectedProposal, setSelectedProposal] = useState<Proposal | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showCreateForm, setShowCreateForm] = useState(false);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const res = await fetch("/api/governance");
      if (!res.ok) throw new Error("Failed to load governance data");
      const json = await res.json();
      setData(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load governance data");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const filteredProposals = data?.proposals.filter((p) => {
    if (filter === "active") return p.status === "active";
    if (filter === "past") return p.status !== "active";
    return true;
  }) ?? [];

  const activeCount = data?.proposals.filter((p) => p.status === "active").length ?? 0;
  const passedCount = data?.proposals.filter((p) => p.status === "passed").length ?? 0;

  const handleVote = async (proposalId: string, vote: "for" | "against") => {
    try {
      const res = await fetch(`/api/governance`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "vote", proposalId, vote }),
      });
      if (!res.ok) throw new Error("Failed to cast vote");
      // Update local state
      if (data) {
        setData({
          ...data,
          proposals: data.proposals.map((p) =>
            p.id === proposalId
              ? { ...p, userVote: vote, votesFor: vote === "for" ? p.votesFor + 1 : p.votesFor, votesAgainst: vote === "against" ? p.votesAgainst + 1 : p.votesAgainst }
              : p,
          ),
        });
      }
    } catch {
      // Silently fail for now
    }
  };

  const handleCreateProposal = async (formData: { title: string; description: string; category: string }) => {
    setIsSubmitting(true);
    try {
      const res = await fetch("/api/governance", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "create", ...formData }),
      });
      if (!res.ok) throw new Error("Failed to create proposal");
      loadData();
    } catch {
      // Silently fail
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="max-w-4xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Governance</h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Participate in Xergon Network governance decisions
          </p>
        </div>
        <button
          onClick={() => setShowCreateForm(!showCreateForm)}
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-brand-700 self-start"
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
          {showCreateForm ? "Cancel" : "Create Proposal"}
        </button>
      </div>

      {/* Error */}
      {error && !isLoading && (
        <div className="mb-6 rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      {/* Voting power card */}
      {isLoading ? (
        <SkeletonPulse className="h-24 w-full mb-6" />
      ) : data ? (
        <ErrorBoundary context="Voting Power">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 shadow-sm mb-6">
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
              <div>
                <div className="text-xs text-surface-800/40">Voting Power</div>
                <div className="text-lg font-bold text-surface-900">
                  {data.votingPower.votingPower.toFixed(2)}
                </div>
              </div>
              <div>
                <div className="text-xs text-surface-800/40">ERG Balance</div>
                <div className="text-lg font-bold text-surface-900">
                  {formatErg(data.votingPower.ergBalance)}
                </div>
              </div>
              <div>
                <div className="text-xs text-surface-800/40">XRG Staked</div>
                <div className="text-lg font-bold text-surface-900">
                  {formatErg(data.votingPower.xrgStaked)}
                </div>
              </div>
              <div>
                <div className="text-xs text-surface-800/40">Delegations</div>
                <div className="text-lg font-bold text-surface-900">
                  {data.votingPower.delegations}
                </div>
              </div>
            </div>
          </div>
        </ErrorBoundary>
      ) : null}

      {/* Create proposal form */}
      {showCreateForm && (
        <div className="mb-6">
          <CreateProposalForm onSubmit={handleCreateProposal} isSubmitting={isSubmitting} />
        </div>
      )}

      {/* Filter tabs */}
      <div className="flex items-center gap-1 mb-6 rounded-lg border border-surface-200 bg-surface-50 p-1">
        {(["active", "past", "all"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setFilter(tab)}
            className={cn(
              "flex-1 rounded-md px-3 py-1.5 text-xs font-medium transition-colors capitalize",
              filter === tab
                ? "bg-surface-0 text-surface-900 shadow-sm"
                : "text-surface-800/50 hover:text-surface-900",
            )}
          >
            {tab === "active" ? `Active (${activeCount})` : tab === "past" ? `Past (${passedCount})` : "All"}
          </button>
        ))}
      </div>

      {/* Proposals list */}
      {isLoading ? (
        <div className="space-y-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <SkeletonPulse key={i} className="h-48 w-full" />
          ))}
        </div>
      ) : filteredProposals.length === 0 ? (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
          <div className="text-sm text-surface-800/40">
            No {filter === "active" ? "active" : filter} proposals found
          </div>
        </div>
      ) : (
        <ErrorBoundary context="Proposals List">
          <div className="space-y-4">
            {filteredProposals.map((proposal) => (
              <ProposalCard
                key={proposal.id}
                proposal={proposal}
                onVote={proposal.status === "active" ? handleVote : undefined}
                onClick={(id) => {
                  const p = data?.proposals.find((pr) => pr.id === id);
                  if (p) setSelectedProposal(p);
                }}
              />
            ))}
          </div>
        </ErrorBoundary>
      )}

      {/* Proposal detail modal */}
      {selectedProposal && (
        <ProposalDetailModal
          proposal={selectedProposal}
          onClose={() => setSelectedProposal(null)}
        />
      )}
    </div>
  );
}
