"use client";

import { useMemo } from "react";
import type { NetworkStats } from "@/lib/api/analytics";

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
  return `${erg.toFixed(1)} ERG`;
}

// ---------------------------------------------------------------------------
// Metric card definition
// ---------------------------------------------------------------------------

interface MetricCard {
  icon: React.ReactNode;
  label: string;
  value: string;
  subLabel: string;
  trend?: "up" | "down" | "neutral";
}

function UpArrow() {
  return (
    <svg className="w-3.5 h-3.5 text-accent-500" viewBox="0 0 20 20" fill="currentColor">
      <path fillRule="evenodd" d="M12 7a1 1 0 10-2 0v3.586L8.707 9.293a1 1 0 00-1.414 1.414l3 3a1 1 0 001.414 0l3-3a1 1 0 00-1.414-1.414L12 10.586V7z" clipRule="evenodd" />
    </svg>
  );
}

function DownArrow() {
  return (
    <svg className="w-3.5 h-3.5 text-danger-500" viewBox="0 0 20 20" fill="currentColor">
      <path fillRule="evenodd" d="M12 13a1 1 0 100-2V7.414L13.293 8.707a1 1 0 001.414-1.414l-3-3a1 1 0 00-1.414 0l-3 3a1 1 0 101.414 1.414L8 7.414V11a1 1 0 100 2h4z" clipRule="evenodd" />
    </svg>
  );
}

function ProviderIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 00-3-3.87" />
      <path d="M16 3.13a4 4 0 010 7.75" />
    </svg>
  );
}

function ModelIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
      <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
      <line x1="12" y1="22.08" x2="12" y2="12" />
    </svg>
  );
}

function StakingIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <path d="M16 8h-6a2 2 0 00-2 2v1a2 2 0 002 2h4a2 2 0 012 2v1a2 2 0 01-2 2H8" />
      <path d="M12 18V6" />
    </svg>
  );
}

function RequestIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
    </svg>
  );
}

function TokenIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
      <polyline points="22 6 12 13 2 6" />
    </svg>
  );
}

function LatencyIcon() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Single metric card
// ---------------------------------------------------------------------------

function MetricCardView({ card, index }: { card: MetricCard; index: number }) {
  return (
    <div
      className="rounded-xl border border-surface-200 bg-surface-0 p-5 transition-all hover:shadow-md"
      role="region"
      aria-label={`${card.label}: ${card.value}${card.subLabel ? `, ${card.subLabel}` : ""}`}
      style={{
        animationDelay: `${index * 80}ms`,
        animation: "fadeIn 0.4s ease-out both",
      }}
    >
      <div className="flex items-start justify-between mb-3">
        <div className="rounded-lg bg-brand-50 p-2 text-brand-600 dark:bg-brand-950/30" aria-hidden="true">
          {card.icon}
        </div>
        {card.trend && card.trend !== "neutral" && (
          <div className="flex items-center gap-0.5 text-xs font-medium" aria-hidden="true">
            {card.trend === "up" ? <UpArrow /> : <DownArrow />}
          </div>
        )}
      </div>
      <div className="text-2xl font-bold text-surface-900 mb-0.5">
        {card.value}
      </div>
      <div className="text-sm text-surface-800/50">{card.label}</div>
      {card.subLabel && (
        <div className="text-xs text-surface-800/30 mt-0.5">{card.subLabel}</div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// StatsHero component
// ---------------------------------------------------------------------------

export function StatsHero({ stats }: { stats: NetworkStats }) {
  const cards = useMemo<MetricCard[]>(() => {
    return [
      {
        icon: <ProviderIcon />,
        label: "Total Providers",
        value: formatNumber(stats.totalProviders),
        subLabel: `${stats.activeProviders} active`,
        trend: "up",
      },
      {
        icon: <ModelIcon />,
        label: "Active Models",
        value: String(stats.activeModels),
        subLabel: "Across all providers",
        trend: "up",
      },
      {
        icon: <StakingIcon />,
        label: "ERG Staked",
        value: nanoergToErg(stats.totalErgStaked),
        subLabel: stats.totalErgStaked > 0 ? "In staking boxes" : "Not available",
        trend: "neutral",
      },
      {
        icon: <RequestIcon />,
        label: "Requests (24h)",
        value: formatNumber(stats.requests24h),
        subLabel: "Total API requests",
        trend: "up",
      },
      {
        icon: <TokenIcon />,
        label: "Tokens Processed",
        value: formatNumber(stats.totalTokensProcessed),
        subLabel: "All-time tokens",
        trend: "up",
      },
      {
        icon: <LatencyIcon />,
        label: "Avg Latency",
        value: `${stats.avgLatencyMs}ms`,
        subLabel: "Across all requests",
        trend: stats.avgLatencyMs < 400 ? "up" : "down",
      },
    ];
  }, [stats]);

  return (
    <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4">
      {cards.map((card, i) => (
        <MetricCardView key={card.label} card={card} index={i} />
      ))}
    </div>
  );
}
