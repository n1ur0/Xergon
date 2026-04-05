"use client";

import { ProviderInfo } from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface ProviderCardProps {
  provider: ProviderInfo;
  expanded: boolean;
  onToggle: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function statusColor(status: ProviderInfo["status"]): string {
  switch (status) {
    case "online":
      return "bg-green-500";
    case "degraded":
      return "bg-yellow-500";
    case "offline":
      return "bg-red-500";
  }
}

function statusBadgeClasses(status: ProviderInfo["status"]): string {
  switch (status) {
    case "online":
      return "bg-green-500/10 text-green-700 dark:text-green-400";
    case "degraded":
      return "bg-yellow-500/10 text-yellow-700 dark:text-yellow-400";
    case "offline":
      return "bg-red-500/10 text-red-700 dark:text-red-400";
  }
}

function uptimeBarColor(uptime: number): string {
  if (uptime >= 95) return "bg-green-500";
  if (uptime >= 85) return "bg-yellow-500";
  return "bg-red-500";
}

function formatNanoErg(nano: number): string {
  if (nano >= 1_000_000_000) return `${(nano / 1_000_000_000).toFixed(2)} ERG`;
  if (nano >= 1_000_000) return `${(nano / 1_000_000).toFixed(1)}mERG`;
  if (nano >= 1_000) return `${(nano / 1_000).toFixed(1)}µERG`;
  return `${nano} nERG`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function regionFlag(region: string): string {
  const flags: Record<string, string> = {
    US: "🇺🇸",
    EU: "🇪🇺",
    Asia: "🌏",
    Other: "🌍",
  };
  return flags[region] ?? "🌍";
}

function truncateEndpoint(endpoint: string): string {
  if (endpoint.length <= 40) return endpoint;
  return `${endpoint.slice(0, 20)}...${endpoint.slice(-17)}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProviderCard({ provider, expanded, onToggle }: ProviderCardProps) {
  const maxVisibleModels = 4;
  const visibleModels = provider.models.slice(0, maxVisibleModels);
  const hiddenCount = provider.models.length - maxVisibleModels;

  return (
    <div
      className={`group rounded-xl border border-surface-200 bg-surface-0 transition-all duration-200 ${
        expanded
          ? "ring-2 ring-brand-500/30 shadow-lg"
          : "hover:shadow-md hover:border-surface-300"
      }`}
    >
      {/* Card header */}
      <button
        type="button"
        onClick={onToggle}
        className="w-full text-left p-4 space-y-3"
        aria-expanded={expanded}
      >
        {/* Top row: name + status */}
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <span
              className={`inline-block h-2.5 w-2.5 rounded-full ${statusColor(provider.status)} shrink-0`}
              aria-hidden="true"
            />
            <h3 className="font-semibold text-surface-900 truncate">
              {provider.name}
            </h3>
          </div>
          <span
            className={`shrink-0 text-xs font-medium px-2 py-0.5 rounded-full capitalize ${statusBadgeClasses(provider.status)}`}
          >
            {provider.status}
          </span>
        </div>

        {/* Endpoint */}
        <p className="text-xs font-mono text-surface-800/50 truncate">
          {truncateEndpoint(provider.endpoint)}
        </p>

        {/* Model tags */}
        {provider.models.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {visibleModels.map((model) => (
              <span
                key={model}
                className="inline-block text-[11px] px-1.5 py-0.5 rounded-md bg-surface-100 text-surface-800/70"
              >
                {model}
              </span>
            ))}
            {hiddenCount > 0 && (
              <span className="inline-block text-[11px] px-1.5 py-0.5 rounded-md bg-brand-500/10 text-brand-600">
                +{hiddenCount} more
              </span>
            )}
          </div>
        )}

        {/* Uptime bar */}
        <div className="space-y-1">
          <div className="flex items-center justify-between text-xs text-surface-800/60">
            <span>Uptime</span>
            <span className="font-medium">{provider.uptime.toFixed(1)}%</span>
          </div>
          <div
            className="h-1.5 w-full rounded-full bg-surface-200 overflow-hidden"
            role="progressbar"
            aria-valuenow={provider.uptime}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-label={`Uptime: ${provider.uptime.toFixed(1)}%`}
          >
            <div
              className={`h-full rounded-full transition-all duration-500 ${uptimeBarColor(provider.uptime)}`}
              style={{ width: `${provider.uptime}%` }}
            />
          </div>
        </div>

        {/* Key metrics row */}
        <div className="grid grid-cols-3 gap-2 text-center">
          <div className="rounded-lg bg-surface-50 p-2">
            <div className="text-xs text-surface-800/50">AI Points</div>
            <div className="text-sm font-semibold text-surface-900">
              {provider.aiPoints.toLocaleString()}
            </div>
          </div>
          <div className="rounded-lg bg-surface-50 p-2">
            <div className="text-xs text-surface-800/50">Tokens</div>
            <div className="text-sm font-semibold text-surface-900">
              {formatTokens(provider.totalTokens)}
            </div>
          </div>
          <div className="rounded-lg bg-surface-50 p-2">
            <div className="text-xs text-surface-800/50">Price/1M</div>
            <div className="text-sm font-semibold text-surface-900">
              {formatNanoErg(provider.pricePer1mTokens)}
            </div>
          </div>
        </div>

        {/* Bottom row: region, GPU, latency */}
        <div className="flex items-center justify-between text-xs text-surface-800/60 pt-1">
          <span className="flex items-center gap-1">
            {regionFlag(provider.region)} {provider.region}
          </span>
          <span className="truncate max-w-[120px]" title={provider.gpuInfo}>
            {provider.gpuInfo}
          </span>
          <span>{provider.latencyMs}ms</span>
        </div>
      </button>

      {/* Expanded section — placeholder content, actual detail rendered by parent */}
      {expanded && (
        <div className="px-4 pb-4 border-t border-surface-200 pt-3 animate-in fade-in slide-in-from-top-2 duration-200">
          {/* This content is replaced by ProviderDetail when expanded */}
        </div>
      )}
    </div>
  );
}
