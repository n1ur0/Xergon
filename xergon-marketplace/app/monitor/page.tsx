"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OnChainBox {
  boxId: string;
  value: number; // nanoERG
  creationHeight: number;
  ergoTree?: string;
  tokens?: Array<{ tokenId: string; amount: number; name?: string }>;
}

interface OnChainProviderResponse {
  providers?: OnChainBox[];
  treasury?: OnChainBox[];
  governance?: OnChainBox[];
  staking?: OnChainBox[];
  [key: string]: OnChainBox[] | undefined;
}

interface BoxGroupSummary {
  label: string;
  key: string;
  icon: string;
  boxes: OnChainBox[];
  boxCount: number;
  totalErg: number;
  oldestBoxAgeBlocks: number;
  oldestBoxId: string;
  rentProtectedUntil: string;
  healthStatus: "green" | "yellow" | "red";
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const REFRESH_INTERVAL_MS = 5 * 60 * 1000; // 5 minutes
const BLOCKS_PER_YEAR = 525600; // ~2 min block time
const STORAGE_RENT_THRESHOLD = 4 * BLOCKS_PER_YEAR; // 2,102,400 blocks = 4 years
const ERG_EXPLORER_BASE = "https://explorer.ergoplatform.com/en/boxes";

// Thresholds for color-coded warnings
const YELLOW_THRESHOLD = 2 * BLOCKS_PER_YEAR;  // 2 years
const RED_THRESHOLD = 3.5 * BLOCKS_PER_YEAR;    // 3.5 years

// Default box groups
const BOX_GROUP_KEYS = [
  { key: "treasury", label: "Treasury Boxes", icon: "vault" },
  { key: "providers", label: "Provider Boxes", icon: "server" },
  { key: "governance", label: "Governance Boxes", icon: "scale" },
  { key: "staking", label: "Staking Boxes", icon: "coins" },
] as const;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function nanoErgToErg(nanoErg: number): number {
  return nanoErg / 1e9;
}

function blocksToYears(blocks: number): number {
  return blocks / BLOCKS_PER_YEAR;
}

function estimateDateFromHeight(creationHeight: number, currentHeight: number): string {
  const rentHeight = creationHeight + STORAGE_RENT_THRESHOLD;
  const blocksUntilRent = rentHeight - currentHeight;
  if (blocksUntilRent <= 0) {
    return "Expired";
  }
  const minutesUntilRent = blocksUntilRent * 2;
  const date = new Date(Date.now() + minutesUntilRent * 60 * 1000);
  return date.toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function getHealthStatus(oldestAgeBlocks: number): "green" | "yellow" | "red" {
  if (oldestAgeBlocks >= RED_THRESHOLD) return "red";
  if (oldestAgeBlocks >= YELLOW_THRESHOLD) return "yellow";
  return "green";
}

function healthColor(status: "green" | "yellow" | "red"): string {
  switch (status) {
    case "green":
      return "text-emerald-600 dark:text-emerald-400";
    case "yellow":
      return "text-amber-600 dark:text-amber-400";
    case "red":
      return "text-red-600 dark:text-red-400";
  }
}

function healthBg(status: "green" | "yellow" | "red"): string {
  switch (status) {
    case "green":
      return "bg-emerald-50 dark:bg-emerald-950/20 border-emerald-200 dark:border-emerald-800/40";
    case "yellow":
      return "bg-amber-50 dark:bg-amber-950/20 border-amber-200 dark:border-amber-800/40";
    case "red":
      return "bg-red-50 dark:bg-red-950/20 border-red-200 dark:border-red-800/40";
  }
}

function healthDot(status: "green" | "yellow" | "red"): string {
  switch (status) {
    case "green":
      return "bg-emerald-500";
    case "yellow":
      return "bg-amber-500";
    case "red":
      return "bg-red-500 animate-pulse";
  }
}

// ---------------------------------------------------------------------------
// Icons (inline SVGs to avoid new deps)
// ---------------------------------------------------------------------------

function ShieldIcon({ className }: { className?: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  );
}

function VaultIcon({ className }: { className?: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M7 15h10M7 11h10" />
      <circle cx="12" cy="11" r="1" />
    </svg>
  );
}

function ServerIcon({ className }: { className?: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <rect x="2" y="2" width="20" height="8" rx="2" />
      <rect x="2" y="14" width="20" height="8" rx="2" />
      <circle cx="6" cy="6" r="1" />
      <circle cx="6" cy="18" r="1" />
    </svg>
  );
}

function ScaleIcon({ className }: { className?: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <line x1="12" y1="3" x2="12" y2="21" />
      <path d="M4 7l4-4 4 4M4 7H2v4h4zM20 7l-4-4-4 4M20 7h2v4h-4z" />
    </svg>
  );
}

function CoinsIcon({ className }: { className?: string }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className} aria-hidden="true">
      <circle cx="8" cy="8" r="6" />
      <circle cx="16" cy="16" r="6" />
      <path d="M12 12h.01" />
    </svg>
  );
}

function ExternalLinkIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
      <polyline points="15 3 21 3 21 9" />
      <line x1="10" y1="14" x2="21" y2="3" />
    </svg>
  );
}

function groupIcon(icon: string, className?: string) {
  switch (icon) {
    case "vault": return <VaultIcon className={className} />;
    case "server": return <ServerIcon className={className} />;
    case "scale": return <ScaleIcon className={className} />;
    case "coins": return <CoinsIcon className={className} />;
    default: return <VaultIcon className={className} />;
  }
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function MonitorSkeleton() {
  return (
    <div className="space-y-6">
      {/* Summary cards skeleton */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <SkeletonPulse className="h-4 w-24 mb-3" />
            <SkeletonPulse className="h-8 w-20 mb-2" />
            <SkeletonPulse className="h-3 w-32" />
          </div>
        ))}
      </div>
      {/* Table skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-100">
          <SkeletonPulse className="h-5 w-40 mb-1" />
          <SkeletonPulse className="h-3 w-56" />
        </div>
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="flex items-center gap-4 px-5 py-4 border-b border-surface-50">
            <SkeletonPulse className="h-5 w-5 rounded" />
            <SkeletonPulse className="h-4 w-28" />
            <div className="flex-1" />
            <SkeletonPulse className="h-4 w-16" />
            <SkeletonPulse className="h-4 w-20" />
            <SkeletonPulse className="h-5 w-20 rounded-full" />
          </div>
        ))}
      </div>
      {/* Education section skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <SkeletonPulse className="h-5 w-48 mb-4" />
        <div className="space-y-2">
          <SkeletonPulse className="h-4 w-full" />
          <SkeletonPulse className="h-4 w-3/4" />
          <SkeletonPulse className="h-4 w-5/6" />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Error state
// ---------------------------------------------------------------------------

function MonitorError({ onRetry }: { onRetry: () => void }) {
  return (
    <div className="rounded-xl border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 p-8 text-center">
      <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/30">
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-red-500" aria-hidden="true">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
      </div>
      <h2 className="text-lg font-semibold text-surface-900 mb-1">Chain Data Unavailable</h2>
      <p className="text-sm text-surface-800/60 mb-4">
        Unable to fetch on-chain box data. The agent service may be temporarily unreachable.
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <polyline points="23 4 23 10 17 10" />
          <polyline points="1 20 1 14 7 14" />
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
        </svg>
        Retry
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Age bar visualization
// ---------------------------------------------------------------------------

function AgeBar({ blocks }: { blocks: number }) {
  const percent = Math.min(100, (blocks / STORAGE_RENT_THRESHOLD) * 100);
  const status = getHealthStatus(blocks);

  const barColor = status === "green"
    ? "bg-emerald-500"
    : status === "yellow"
      ? "bg-amber-500"
      : "bg-red-500";

  return (
    <div className="w-full h-2 rounded-full bg-surface-100 dark:bg-surface-800 overflow-hidden">
      <div
        className={`h-full rounded-full transition-all duration-500 ${barColor}`}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Summary card for a box group
// ---------------------------------------------------------------------------

function GroupSummaryCard({ summary }: { summary: BoxGroupSummary }) {
  return (
    <div className={`rounded-xl border p-5 ${healthBg(summary.healthStatus)}`}>
      <div className="flex items-center gap-2 mb-3">
        <span className={healthColor(summary.healthStatus)}>
          {groupIcon(summary.icon)}
        </span>
        <h3 className="text-sm font-semibold text-surface-900">{summary.label}</h3>
      </div>
      <div className="space-y-1.5">
        <p className="text-2xl font-bold text-surface-900">
          {summary.boxCount} <span className="text-sm font-normal text-surface-800/50">boxes</span>
        </p>
        <p className="text-xs text-surface-800/60">
          {summary.totalErg.toFixed(2)} ERG locked
        </p>
        <p className="text-xs text-surface-800/60">
          Oldest: {blocksToYears(summary.oldestBoxAgeBlocks).toFixed(1)} years
        </p>
      </div>
      <div className="mt-3">
        <AgeBar blocks={summary.oldestBoxAgeBlocks} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Box table row
// ---------------------------------------------------------------------------

function BoxRow({
  boxId,
  value,
  creationHeight,
  currentHeight,
}: {
  boxId: string;
  value: number;
  creationHeight: number;
  currentHeight: number;
}) {
  const age = currentHeight - creationHeight;
  const status = getHealthStatus(age);
  const ergValue = nanoErgToErg(value);

  return (
    <tr className="border-b border-surface-50 hover:bg-surface-50 dark:hover:bg-surface-900/30 transition-colors">
      <td className="px-5 py-3">
        <span className={`inline-flex items-center gap-1.5`}>
          <span className={`h-2 w-2 rounded-full ${healthDot(status)}`} aria-hidden="true" />
          <span className={`text-xs font-medium ${healthColor(status)}`}>
            {blocksToYears(age).toFixed(1)}y
          </span>
        </span>
      </td>
      <td className="px-3 py-3 text-xs font-mono">
        <a
          href={`${ERG_EXPLORER_BASE}/${boxId}`}
          target="_blank"
          rel="noopener noreferrer"
          className="text-brand-600 hover:text-brand-700 transition-colors inline-flex items-center gap-1"
        >
          {boxId.slice(0, 12)}...{boxId.slice(-6)}
          <ExternalLinkIcon />
        </a>
      </td>
      <td className="px-3 py-3 text-xs text-surface-800/60 text-right">
        {ergValue.toFixed(4)} ERG
      </td>
      <td className="px-3 py-3 text-xs text-surface-800/60 text-right">
        {creationHeight.toLocaleString()}
      </td>
      <td className="px-5 py-3 text-xs text-surface-800/60 text-right">
        {estimateDateFromHeight(creationHeight, currentHeight)}
      </td>
    </tr>
  );
}

// ---------------------------------------------------------------------------
// Storage rent education section
// ---------------------------------------------------------------------------

function StorageRentEducation() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <div className="flex items-center gap-2 mb-4">
        <ShieldIcon className="text-brand-600" />
        <h2 className="text-base font-semibold text-surface-900">
          How Storage Rent Works
        </h2>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="space-y-3">
          <p className="text-xs text-surface-800/60 leading-relaxed">
            Ergo implements a unique economic model called <strong className="text-surface-900">storage rent</strong>.
            Unlike most blockchains where unspent outputs (boxes) remain on-chain indefinitely,
            Ergo charges a fee for storing data long-term.
          </p>
          <p className="text-xs text-surface-800/60 leading-relaxed">
            After <strong className="text-surface-900">4 years</strong> (approximately 1,051,200 blocks at 2-minute block times),
            miners can collect a storage fee from unspent boxes. The fee is roughly{" "}
            <strong className="text-surface-900">0.14 ERG per 4-year period</strong> for a minimal box.
          </p>
        </div>
        <div className="space-y-3">
          <p className="text-xs text-surface-800/60 leading-relaxed">
            This mechanism incentivizes active chain management and prevents blockchain bloat.
            Protocol boxes that hold ERG or tokens must be periodically refreshed (spent and re-created)
            to prevent value loss.
          </p>
          <div className="rounded-lg bg-surface-50 dark:bg-surface-900/50 p-3 space-y-2">
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 rounded-full bg-emerald-500" aria-hidden="true" />
              <span className="text-xs text-surface-800/70"><strong>Green</strong>: &lt; 2 years old — safe</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 rounded-full bg-amber-500" aria-hidden="true" />
              <span className="text-xs text-surface-800/70"><strong>Yellow</strong>: 2–3.5 years — plan refresh</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 rounded-full bg-red-500" aria-hidden="true" />
              <span className="text-xs text-surface-800/70"><strong>Red</strong>: &gt; 3.5 years — urgent action needed</span>
            </div>
          </div>
          <a
            href="https://docs.ergoplatform.com/ergo/storage-rent/"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-xs text-brand-600 hover:text-brand-700 transition-colors"
          >
            Learn more about Ergo storage rent
            <ExternalLinkIcon />
          </a>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function MonitorPage() {
  const [boxGroups, setBoxGroups] = useState<BoxGroupSummary[]>([]);
  const [currentHeight, setCurrentHeight] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [isError, setIsError] = useState(false);
  const [lastRefresh, setLastRefresh] = useState<string>("");
  const [expandedGroup, setExpandedGroup] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setIsError(false);

    try {
      const res = await fetch("/api/xergon-agent/api/providers/on-chain", {
        cache: "no-store",
      });
      if (!res.ok) throw new Error(`On-chain endpoint returned ${res.status}`);
      const data: OnChainProviderResponse = await res.json();

      // Estimate current height from the oldest box creation heights + some buffer
      // In production, this would come from the chain status endpoint
      let maxCreationHeight = 0;
      for (const groupKey of BOX_GROUP_KEYS) {
        const boxes = data[groupKey.key] ?? [];
        for (const box of boxes) {
          if (box.creationHeight > maxCreationHeight) {
            maxCreationHeight = box.creationHeight;
          }
        }
      }
      // Assume chain is ~100000 blocks ahead of the oldest known box
      // This is a rough estimate — in production use the actual chain height
      const estimatedHeight = maxCreationHeight > 0 ? maxCreationHeight + 100000 : 0;

      // Also try to get actual chain height from health endpoint
      let chainHeight = estimatedHeight;
      try {
        const healthRes = await fetch("/api/xergon-relay/health", { cache: "no-store" });
        if (healthRes.ok) {
          const healthData = await healthRes.json();
          if (healthData.chainHeight > 0) {
            chainHeight = healthData.chainHeight;
          }
        }
      } catch {
        // Use estimated height
      }

      setCurrentHeight(chainHeight);

      // Build summaries
      const summaries: BoxGroupSummary[] = BOX_GROUP_KEYS.map((group) => {
        const boxes = data[group.key] ?? [];
        const boxCount = boxes.length;
        const totalErg = boxes.reduce((sum, b) => sum + nanoErgToErg(b.value), 0);

        let oldestBoxAgeBlocks = 0;
        let oldestBoxId = "";
        for (const box of boxes) {
          const age = chainHeight - box.creationHeight;
          if (age > oldestBoxAgeBlocks) {
            oldestBoxAgeBlocks = age;
            oldestBoxId = box.boxId;
          }
        }

        const rentProtectedUntil = oldestBoxId
          ? estimateDateFromHeight(
              boxes.find((b) => b.boxId === oldestBoxId)?.creationHeight ?? 0,
              chainHeight
            )
          : "N/A";

        return {
          label: group.label,
          key: group.key,
          icon: group.icon,
          boxes,
          boxCount,
          totalErg,
          oldestBoxAgeBlocks,
          oldestBoxId,
          rentProtectedUntil,
          healthStatus: getHealthStatus(oldestBoxAgeBlocks),
        };
      });

      setBoxGroups(summaries);
      setLastRefresh(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
      setIsError(false);
    } catch {
      setIsError(true);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Initial load + auto-refresh
  useEffect(() => {
    loadData();
    timerRef.current = setInterval(loadData, REFRESH_INTERVAL_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [loadData]);

  // Overall health
  const overallHealth = boxGroups.length > 0
    ? (boxGroups.some((g) => g.healthStatus === "red")
        ? "red"
        : boxGroups.some((g) => g.healthStatus === "yellow")
          ? "yellow"
          : "green")
    : "green";

  const totalBoxes = boxGroups.reduce((sum, g) => sum + g.boxCount, 0);
  const totalErgLocked = boxGroups.reduce((sum, g) => sum + g.totalErg, 0);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 flex items-center gap-2">
            <ShieldIcon className="text-brand-600" />
            Storage Rent Monitor
          </h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Protocol box health and storage rent tracking
          </p>
        </div>
        <div className="flex items-center gap-3">
          {lastRefresh && (
            <span className="text-xs text-surface-800/40">
              Updated: {lastRefresh}
            </span>
          )}
          <span className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium ${
            overallHealth === "green"
              ? "border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 text-emerald-700 dark:text-emerald-400"
              : overallHealth === "yellow"
                ? "border-amber-200 bg-amber-50 dark:border-amber-800/40 dark:bg-amber-950/20 text-amber-700 dark:text-amber-400"
                : "border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 text-red-700 dark:text-red-400"
          }`}>
            <span className={`h-1.5 w-1.5 rounded-full ${healthDot(overallHealth)}`} aria-hidden="true" />
            {overallHealth === "green" ? "All Healthy" : overallHealth === "yellow" ? "Attention Needed" : "Action Required"}
          </span>
        </div>
      </div>

      <SuspenseWrap fallback={<MonitorSkeleton />}>
        {isError && !isLoading && boxGroups.length === 0 ? (
          <MonitorError onRetry={loadData} />
        ) : isLoading && boxGroups.length === 0 ? (
          <MonitorSkeleton />
        ) : (
          <>
            {/* Summary stats */}
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6">
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
                <p className="text-xs text-surface-800/50 mb-1">Total Boxes</p>
                <p className="text-xl font-bold text-surface-900">{totalBoxes}</p>
              </div>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
                <p className="text-xs text-surface-800/50 mb-1">Total ERG Locked</p>
                <p className="text-xl font-bold text-surface-900">{totalErgLocked.toFixed(2)}</p>
              </div>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
                <p className="text-xs text-surface-800/50 mb-1">Chain Height</p>
                <p className="text-xl font-bold text-surface-900">
                  {currentHeight > 0 ? currentHeight.toLocaleString() : "--"}
                </p>
              </div>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
                <p className="text-xs text-surface-800/50 mb-1">Rent Threshold</p>
                <p className="text-xl font-bold text-surface-900">4 years</p>
              </div>
            </div>

            {/* Group summary cards */}
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
              {boxGroups.map((group) => (
                <GroupSummaryCard key={group.key} summary={group} />
              ))}
            </div>

            {/* Expandable box tables per group */}
            {boxGroups.map((group) => (
              <div
                key={group.key}
                className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden mb-4"
              >
                <button
                  onClick={() => setExpandedGroup(expandedGroup === group.key ? null : group.key)}
                  className="w-full flex items-center justify-between px-5 py-4 text-left hover:bg-surface-50 dark:hover:bg-surface-900/30 transition-colors"
                  aria-expanded={expandedGroup === group.key}
                >
                  <div className="flex items-center gap-2">
                    <span className={healthColor(group.healthStatus)}>
                      {groupIcon(group.icon)}
                    </span>
                    <h2 className="text-sm font-semibold text-surface-900">{group.label}</h2>
                    <span className="text-xs text-surface-800/40">({group.boxCount})</span>
                  </div>
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className={`text-surface-800/40 transition-transform ${expandedGroup === group.key ? "rotate-180" : ""}`}
                    aria-hidden="true"
                  >
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                </button>

                {expandedGroup === group.key && (
                  <div className="border-t border-surface-100">
                    {group.boxes.length === 0 ? (
                      <div className="px-5 py-6 text-center text-xs text-surface-800/40">
                        No boxes found for this category.
                      </div>
                    ) : (
                      <div className="overflow-x-auto">
                        <table className="w-full text-sm">
                          <thead>
                            <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100">
                              <th className="px-5 py-2.5 font-medium">Age</th>
                              <th className="px-3 py-2.5 font-medium">Box ID</th>
                              <th className="px-3 py-2.5 font-medium text-right">Value</th>
                              <th className="px-3 py-2.5 font-medium text-right">Created At</th>
                              <th className="px-5 py-2.5 font-medium text-right">Rent Protected Until</th>
                            </tr>
                          </thead>
                          <tbody>
                            {/* Sort by oldest first */}
                            {group.boxes
                              .sort((a, b) => a.creationHeight - b.creationHeight)
                              .map((box) => (
                                <BoxRow
                                  key={box.boxId}
                                  boxId={box.boxId}
                                  value={box.value}
                                  creationHeight={box.creationHeight}
                                  currentHeight={currentHeight}
                                />
                              ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))}

            {/* Storage rent education */}
            <StorageRentEducation />
          </>
        )}
      </SuspenseWrap>
    </div>
  );
}
