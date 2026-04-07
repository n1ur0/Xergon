"use client";

import { useState } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import type { ProviderInfo } from "@/lib/api/chain";

// ── Types ──

interface ProviderDetailClientProps {
  provider: ProviderInfo;
}

interface Review {
  id: string;
  author: string;
  rating: number;
  content: string;
  date: string;
}

interface ActivityEvent {
  id: string;
  type: "model_added" | "price_change" | "status_change" | "milestone";
  description: string;
  date: string;
}

// ── Helpers ──

function formatNanoErg(nano: number): string {
  if (nano >= 1_000_000_000) return `${(nano / 1_000_000_000).toFixed(2)} ERG`;
  if (nano >= 1_000_000) return `${(nano / 1_000_000).toFixed(1)}mERG`;
  if (nano >= 1_000) return `${(nano / 1_000).toFixed(1)}\u00B5ERG`;
  return `${nano} nERG`;
}

function regionFlag(region: string): string {
  const flags: Record<string, string> = {
    US: "\u{1F1FA}\u{1F1F8}",
    EU: "\u{1F1EA}\u{1F1FA}",
    Asia: "\u{1F30F}",
    Other: "\u{1F30D}",
  };
  return flags[region] ?? "\u{1F30D}";
}

// Mock reviews for demo
const MOCK_REVIEWS: Review[] = [
  {
    id: "1",
    author: "0x3f8a...b2c1",
    rating: 5,
    content: "Excellent uptime and fast response times. Highly recommended for production workloads.",
    date: "2025-12-15",
  },
  {
    id: "2",
    author: "0x7d2e...f4a9",
    rating: 4,
    content: "Good provider overall. Occasional latency spikes during peak hours but generally reliable.",
    date: "2025-12-10",
  },
];

// Mock activity for demo
const MOCK_ACTIVITY: ActivityEvent[] = [
  {
    id: "1",
    type: "model_added",
    description: "Added new model: llama-3.3-70b",
    date: "2025-12-18",
  },
  {
    id: "2",
    type: "price_change",
    description: "Reduced pricing for mistral-small-24b by 15%",
    date: "2025-12-15",
  },
  {
    id: "3",
    type: "milestone",
    description: "Reached 100,000 total requests served",
    date: "2025-12-12",
  },
  {
    id: "4",
    type: "status_change",
    description: "Provider status: online",
    date: "2025-12-10",
  },
];

const ACTIVITY_ICONS: Record<ActivityEvent["type"], string> = {
  model_added: "\u{1F680}",
  price_change: "\u{1F4B0}",
  status_change: "\u{1F7E2}",
  milestone: "\u{1F3C6}",
};

// ── Star Rating ──

function StarRating({ rating }: { rating: number }) {
  return (
    <div className="flex items-center gap-0.5">
      {[1, 2, 3, 4, 5].map((star) => (
        <svg
          key={star}
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill={star <= rating ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="2"
          className={star <= rating ? "text-amber-400" : "text-surface-300"}
        >
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      ))}
    </div>
  );
}

// ── Component ──

export function ProviderDetailClient({ provider }: ProviderDetailClientProps) {
  const displayName =
    provider.provider_id.length > 16
      ? `${provider.provider_id.slice(0, 12)}...${provider.provider_id.slice(-6)}`
      : provider.provider_id;

  const isOnline = provider.is_active && provider.healthy;
  const avgRating = 4.5; // Mock average

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Back link */}
      <Link
        href="/providers"
        className="inline-flex items-center gap-1.5 text-sm text-surface-800/50 hover:text-surface-800/80 mb-6 transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="m12 19-7-7 7-7" />
          <path d="M19 12H5" />
        </svg>
        Back to Providers
      </Link>

      {/* Provider Header */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
        <div className="flex flex-col sm:flex-row sm:items-start gap-4">
          {/* Avatar */}
          <div className="flex items-center justify-center h-16 w-16 rounded-full bg-brand-100 text-brand-700 text-2xl font-bold shrink-0">
            {displayName.slice(0, 2).toUpperCase()}
          </div>

          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap mb-1">
              <h1 className="text-xl font-bold text-surface-900">{displayName}</h1>
              <span
                className={cn(
                  "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium",
                  isOnline
                    ? "bg-green-500/10 text-green-700"
                    : "bg-red-500/10 text-red-700",
                )}
              >
                <span className={cn("h-1.5 w-1.5 rounded-full", isOnline ? "bg-green-500" : "bg-red-500")} />
                {isOnline ? "Online" : "Offline"}
              </span>
            </div>

            <p className="text-xs text-surface-800/40 font-mono mb-3">
              {provider.provider_id}
            </p>

            <div className="flex items-center gap-4 text-sm text-surface-800/60">
              <span className="flex items-center gap-1">
                {regionFlag(provider.region)} {provider.region}
              </span>
              <span className="flex items-center gap-1">
                <StarRating rating={Math.round(avgRating)} />
                <span className="font-medium">{avgRating.toFixed(1)}</span>
              </span>
              <span>Reputation: {provider.pown_score.toFixed(1)}</span>
            </div>
          </div>

          {/* Action buttons */}
          <div className="flex gap-2 shrink-0">
            <button className="rounded-lg border border-surface-200 px-3 py-2 text-xs font-medium text-surface-800/60 hover:bg-surface-50 transition-colors">
              Contact
            </button>
            <button className="rounded-lg border border-red-200 px-3 py-2 text-xs font-medium text-red-600 hover:bg-red-50 transition-colors">
              Report
            </button>
          </div>
        </div>
      </div>

      {/* Stats Row */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6">
        <StatCard label="Models Offered" value={String(provider.models.length)} />
        <StatCard
          label="Value Staked"
          value={formatNanoErg(provider.value_nanoerg)}
        />
        <StatCard
          label="Avg Latency"
          value={provider.latency_ms != null ? `${provider.latency_ms}ms` : "N/A"}
        />
        <StatCard
          label="Status"
          value={isOnline ? "Healthy" : "Degraded"}
          valueColor={isOnline ? "text-green-600" : "text-red-600"}
        />
      </div>

      {/* Models Section */}
      <section className="mb-8">
        <h2 className="text-lg font-semibold text-surface-900 mb-4">Models Served</h2>
        {provider.models.length === 0 ? (
          <div className="text-sm text-surface-800/40 py-8 text-center">
            No models currently registered.
          </div>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {provider.models.map((model) => (
              <Link
                key={model}
                href={`/models/${encodeURIComponent(model)}`}
                className="group rounded-lg border border-surface-200 bg-surface-0 p-4 hover:border-brand-300 hover:shadow-sm transition-all"
              >
                <h3 className="font-medium text-surface-900 group-hover:text-brand-600 transition-colors">
                  {model}
                </h3>
                <p className="text-xs text-surface-800/40 mt-1">
                  Served by this provider
                </p>
              </Link>
            ))}
          </div>
        )}
      </section>

      {/* Reviews + Activity */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Reviews */}
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Reviews</h2>
          <div className="space-y-4">
            {MOCK_REVIEWS.map((review) => (
              <div
                key={review.id}
                className="rounded-lg border border-surface-200 bg-surface-0 p-4"
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-mono text-surface-800/60">
                      {review.author}
                    </span>
                    <StarRating rating={review.rating} />
                  </div>
                  <span className="text-xs text-surface-800/30">{review.date}</span>
                </div>
                <p className="text-sm text-surface-800/70">{review.content}</p>
              </div>
            ))}
          </div>
        </section>

        {/* Activity Timeline */}
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Activity</h2>
          <div className="relative pl-6">
            {/* Vertical line */}
            <div className="absolute left-2 top-2 bottom-2 w-px bg-surface-200" />

            <div className="space-y-4">
              {MOCK_ACTIVITY.map((event) => (
                <div key={event.id} className="relative">
                  {/* Dot */}
                  <div className="absolute -left-4 top-1.5 h-3 w-3 rounded-full bg-surface-200 border-2 border-surface-0" />
                  <div className="rounded-lg border border-surface-200 bg-surface-0 p-3">
                    <div className="flex items-center gap-2 mb-1">
                      <span>{ACTIVITY_ICONS[event.type]}</span>
                      <span className="text-xs text-surface-800/40">{event.date}</span>
                    </div>
                    <p className="text-sm text-surface-800/70">{event.description}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

// ── Stat Card ──

function StatCard({
  label,
  value,
  valueColor,
}: {
  label: string;
  value: string;
  valueColor?: string;
}) {
  return (
    <div className="rounded-lg border border-surface-200 bg-surface-0 p-4">
      <div className="text-xs text-surface-800/40 mb-1">{label}</div>
      <div className={cn("text-lg font-semibold", valueColor ?? "text-surface-900")}>
        {value}
      </div>
    </div>
  );
}
