"use client";

import { useState } from "react";
import type { ProviderInfo } from "@/lib/api/providers";
import { generateSparkline, generateSparklineArea } from "@/lib/utils/charts";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface ProviderDetailProps {
  provider: ProviderInfo;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNanoErg(nano: number): string {
  if (nano >= 1_000_000_000) return `${(nano / 1_000_000_000).toFixed(2)} ERG`;
  if (nano >= 1_000_000) return `${(nano / 1_000_000).toFixed(1)} mERG`;
  if (nano >= 1_000) return `${(nano / 1_000).toFixed(1)} µERG`;
  return `${nano} nERG`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProviderDetail({ provider }: ProviderDetailProps) {
  const [copied, setCopied] = useState(false);

  // Uptime history sparkline
  const uptimeHistory = provider.uptimeHistory ?? Array.from({ length: 7 }, () => provider.uptime);
  const sparkW = 200;
  const sparkH = 40;
  const sparkLine = generateSparkline(uptimeHistory, sparkW, sparkH);
  const sparkArea = generateSparklineArea(uptimeHistory, sparkW, sparkH);

  const handleCopyEndpoint = async () => {
    try {
      await navigator.clipboard.writeText(provider.endpoint);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback
      const ta = document.createElement("textarea");
      ta.value = provider.endpoint;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  // Score breakdown (simulated)
  const scoreComponents = [
    { label: "Uptime", value: provider.uptime / 100, weight: 0.3 },
    { label: "AI Points", value: Math.min(provider.aiPoints / 5000, 1), weight: 0.25 },
    { label: "Speed", value: Math.max(0, 1 - provider.latencyMs / 500), weight: 0.25 },
    { label: "Models", value: Math.min(provider.models.length / 4, 1), weight: 0.1 },
    { label: "Price", value: Math.max(0, 1 - provider.pricePer1mTokens / 250_000), weight: 0.1 },
  ];

  const totalScore = scoreComponents.reduce((s, c) => s + c.value * c.weight, 0);

  return (
    <div className="space-y-4 animate-in fade-in slide-in-from-top-2 duration-200">
      {/* Copy endpoint button */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleCopyEndpoint}
          className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-1.5 rounded-lg bg-brand-500/10 text-brand-600 hover:bg-brand-500/20 transition-colors"
        >
          {copied ? (
            <>
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="20 6 9 17 4 12" />
              </svg>
              Copied!
            </>
          ) : (
            <>
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
              </svg>
              Copy endpoint
            </>
          )}
        </button>
        <span className="text-xs text-surface-800/40">
          Last seen: {timeAgo(provider.lastSeen)}
        </span>
      </div>

      {/* Ergo address */}
      {provider.ergoAddress && (
        <div className="text-xs font-mono text-surface-800/40">
          Ergo: {provider.ergoAddress}
        </div>
      )}

      {/* Uptime sparkline (last 7 days) */}
      <div className="space-y-1">
        <h4 className="text-xs font-medium text-surface-800/70">Uptime (Last 7 Days)</h4>
        <svg width={sparkW} height={sparkH} viewBox={`0 0 ${sparkW} ${sparkH}`} className="w-full">
          <path d={sparkArea} fill="rgba(34,197,94,0.1)" />
          <path d={sparkLine} fill="none" stroke="rgb(34,197,94)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <div className="flex justify-between text-[10px] text-surface-800/40">
          <span>7 days ago</span>
          <span>Today</span>
        </div>
      </div>

      {/* Provider score breakdown */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <h4 className="text-xs font-medium text-surface-800/70">Provider Score</h4>
          <span className="text-sm font-bold text-surface-900">{(totalScore * 100).toFixed(0)}/100</span>
        </div>
        {scoreComponents.map((comp) => (
          <div key={comp.label} className="space-y-0.5">
            <div className="flex items-center justify-between text-xs text-surface-800/60">
              <span>{comp.label}</span>
              <span>{(comp.value * 100).toFixed(0)}%</span>
            </div>
            <div className="h-1 w-full rounded-full bg-surface-200 overflow-hidden">
              <div
                className="h-full rounded-full bg-brand-500 transition-all duration-300"
                style={{ width: `${comp.value * 100}%` }}
              />
            </div>
          </div>
        ))}
      </div>

      {/* Full model list with per-model pricing */}
      <div className="space-y-1">
        <h4 className="text-xs font-medium text-surface-800/70">
          Models ({provider.models.length})
        </h4>
        <div className="space-y-1">
          {provider.models.map((model) => {
            const price =
              provider.modelPricing?.[model] ?? provider.pricePer1mTokens;
            return (
              <div
                key={model}
                className="flex items-center justify-between rounded-lg bg-surface-50 px-3 py-2"
              >
                <span className="text-xs font-medium text-surface-900">
                  {model}
                </span>
                <span className="text-xs text-surface-800/60">
                  {formatNanoErg(price)}/1M
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Additional stats */}
      <div className="grid grid-cols-2 gap-2 text-xs text-surface-800/60">
        <div className="rounded-lg bg-surface-50 p-2">
          <span className="block text-surface-800/40">Total Tokens</span>
          <span className="font-semibold text-surface-900">
            {formatTokens(provider.totalTokens)}
          </span>
        </div>
        <div className="rounded-lg bg-surface-50 p-2">
          <span className="block text-surface-800/40">Latency</span>
          <span className="font-semibold text-surface-900">
            {provider.latencyMs}ms
          </span>
        </div>
        <div className="rounded-lg bg-surface-50 p-2">
          <span className="block text-surface-800/40">GPU</span>
          <span className="font-semibold text-surface-900">
            {provider.gpuInfo}
          </span>
        </div>
        <div className="rounded-lg bg-surface-50 p-2">
          <span className="block text-surface-800/40">Region</span>
          <span className="font-semibold text-surface-900">
            {provider.region}
          </span>
        </div>
      </div>
    </div>
  );
}
