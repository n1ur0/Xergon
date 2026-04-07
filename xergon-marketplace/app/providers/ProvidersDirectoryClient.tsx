"use client";

import { useState, useMemo } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import { SkeletonCardGrid } from "@/components/ui/SkeletonCard";
import { EmptyState } from "@/components/ui/EmptyState";
import type { ProviderInfo } from "@/lib/api/chain";

// ── Types ──

interface ProvidersDirectoryClientProps {
  providers: ProviderInfo[];
}

type SortField = "name" | "models" | "latency" | "value";
type SortOrder = "asc" | "desc";
type StatusFilter = "all" | "online" | "offline";

// ── Helpers ──

function regionFlag(region: string): string {
  const flags: Record<string, string> = {
    US: "\u{1F1FA}\u{1F1F8}",
    EU: "\u{1F1EA}\u{1F1FA}",
    Asia: "\u{1F30F}",
    Other: "\u{1F30D}",
  };
  return flags[region] ?? "\u{1F30D}";
}

function formatNanoErg(nano: number): string {
  if (nano >= 1_000_000_000) return `${(nano / 1_000_000_000).toFixed(2)} ERG`;
  if (nano >= 1_000_000) return `${(nano / 1_000_000).toFixed(1)}mERG`;
  if (nano >= 1_000) return `${(nano / 1_000).toFixed(1)}\u00B5ERG`;
  return `${nano} nERG`;
}

function truncatePk(pk: string): string {
  if (pk.length <= 16) return pk;
  return `${pk.slice(0, 8)}...${pk.slice(-6)}`;
}

// ── Component ──

export function ProvidersDirectoryClient({ providers }: ProvidersDirectoryClientProps) {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [sortField, setSortField] = useState<SortField>("value");
  const [sortOrder, setSortOrder] = useState<SortOrder>("desc");

  const filtered = useMemo(() => {
    let result = [...providers];

    // Search
    if (search.trim()) {
      const q = search.toLowerCase();
      result = result.filter(
        (p) =>
          p.provider_id.toLowerCase().includes(q) ||
          p.endpoint.toLowerCase().includes(q) ||
          p.models.some((m) => m.toLowerCase().includes(q)),
      );
    }

    // Status
    if (statusFilter === "online") {
      result = result.filter((p) => p.is_active && p.healthy);
    } else if (statusFilter === "offline") {
      result = result.filter((p) => !p.is_active || !p.healthy);
    }

    // Sort
    const dir = sortOrder === "asc" ? 1 : -1;
    result.sort((a, b) => {
      switch (sortField) {
        case "name":
          return a.provider_id.localeCompare(b.provider_id) * dir;
        case "models":
          return (a.models.length - b.models.length) * dir;
        case "latency":
          return ((a.latency_ms ?? 9999) - (b.latency_ms ?? 9999)) * dir;
        case "value":
          return (a.value_nanoerg - b.value_nanoerg) * dir;
        default:
          return 0;
      }
    });

    return result;
  }, [providers, search, statusFilter, sortField, sortOrder]);

  const regions = useMemo(() => {
    const set = new Set(providers.map((p) => p.region));
    return Array.from(set).sort();
  }, [providers]);

  const allModels = useMemo(() => {
    const set = new Set<string>();
    for (const p of providers) {
      for (const m of p.models) set.add(m);
    }
    return Array.from(set).sort();
  }, [providers]);

  return (
    <div className="space-y-6">
      {/* Search + Filters */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="flex-1">
          <input
            type="text"
            placeholder="Search by name, public key, or model..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
          />
        </div>
        <div className="flex gap-2 flex-wrap">
          {/* Status filter */}
          {(["all", "online", "offline"] as const).map((s) => (
            <button
              key={s}
              onClick={() => setStatusFilter(s)}
              className={cn(
                "rounded-full px-3 py-1.5 text-xs font-medium transition-colors capitalize",
                statusFilter === s
                  ? "bg-surface-900 text-white"
                  : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
              )}
            >
              {s}
            </button>
          ))}
        </div>
      </div>

      {/* Sort */}
      <div className="flex items-center gap-3 text-xs text-surface-800/50">
        <span>Sort by:</span>
        {(
          [
            ["value", "Value Staked"],
            ["models", "Models"],
            ["latency", "Latency"],
            ["name", "Name"],
          ] as const
        ).map(([field, label]) => (
          <button
            key={field}
            onClick={() => {
              if (sortField === field) {
                setSortOrder((o) => (o === "asc" ? "desc" : "asc"));
              } else {
                setSortField(field);
                setSortOrder(field === "name" ? "asc" : "desc");
              }
            }}
            className={cn(
              "rounded px-2 py-1 transition-colors",
              sortField === field
                ? "bg-brand-50 text-brand-700 font-medium"
                : "hover:bg-surface-100",
            )}
          >
            {label}
            {sortField === field && (sortOrder === "asc" ? " \u2191" : " \u2193")}
          </button>
        ))}
        <span className="ml-auto">{filtered.length} provider{filtered.length !== 1 ? "s" : ""}</span>
      </div>

      {/* Provider Grid */}
      {filtered.length === 0 ? (
        <EmptyState
          type="no-providers"
          action={
            search || statusFilter !== "all"
              ? { label: "Clear Filters", onClick: () => { setSearch(""); setStatusFilter("all"); } }
              : undefined
          }
        />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((provider) => (
            <Link
              key={provider.provider_id}
              href={`/providers/${encodeURIComponent(provider.provider_id)}`}
              className="group rounded-xl border border-surface-200 bg-surface-0 p-5 transition-all hover:shadow-md hover:border-brand-300"
            >
              {/* Status + Name */}
              <div className="flex items-start justify-between mb-3">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 mb-1">
                    <span
                      className={cn(
                        "h-2.5 w-2.5 rounded-full shrink-0",
                        provider.is_active && provider.healthy
                          ? "bg-green-500 shadow-[0_0_4px_rgba(34,197,94,0.5)]"
                          : "bg-red-400",
                      )}
                    />
                    <h2 className="font-semibold text-surface-900 truncate">
                      {truncatePk(provider.provider_id)}
                    </h2>
                  </div>
                  <p className="text-xs text-surface-800/40 font-mono truncate">
                    {provider.endpoint}
                  </p>
                </div>
              </div>

              {/* Models */}
              <div className="flex flex-wrap gap-1 mb-3">
                {provider.models.slice(0, 3).map((m) => (
                  <span
                    key={m}
                    className="inline-block text-[11px] px-1.5 py-0.5 rounded-md bg-surface-100 text-surface-800/70"
                  >
                    {m}
                  </span>
                ))}
                {provider.models.length > 3 && (
                  <span className="inline-block text-[11px] px-1.5 py-0.5 rounded-md bg-brand-500/10 text-brand-600">
                    +{provider.models.length - 3}
                  </span>
                )}
              </div>

              {/* Stats grid */}
              <div className="grid grid-cols-3 gap-2 mb-3">
                <div className="rounded-lg bg-surface-50 p-2 text-center">
                  <div className="text-[10px] text-surface-800/40">Models</div>
                  <div className="text-sm font-semibold text-surface-900">{provider.models.length}</div>
                </div>
                <div className="rounded-lg bg-surface-50 p-2 text-center">
                  <div className="text-[10px] text-surface-800/40">Latency</div>
                  <div className="text-sm font-semibold text-surface-900">
                    {provider.latency_ms != null ? `${provider.latency_ms}ms` : "N/A"}
                  </div>
                </div>
                <div className="rounded-lg bg-surface-50 p-2 text-center">
                  <div className="text-[10px] text-surface-800/40">Value</div>
                  <div className="text-sm font-semibold text-surface-900">
                    {formatNanoErg(provider.value_nanoerg)}
                  </div>
                </div>
              </div>

              {/* Footer */}
              <div className="flex items-center justify-between text-xs text-surface-800/40 pt-3 border-t border-surface-100">
                <span className="flex items-center gap-1">
                  {regionFlag(provider.region)} {provider.region}
                </span>
                <span className="text-surface-800/20 group-hover:text-brand-600 transition-colors">
                  View Details &rarr;
                </span>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
