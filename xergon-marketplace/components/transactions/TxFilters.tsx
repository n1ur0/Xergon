"use client";

import { useState, useCallback } from "react";

export type TxTypeFilter = "all" | "staking" | "settlement" | "inference_payment" | "reward";
export type TxStatusFilter = "all" | "confirmed" | "pending" | "failed";
export type TxSortOption = "newest" | "oldest" | "amount_high" | "amount_low";

export interface TxFiltersState {
  type: TxTypeFilter;
  status: TxStatusFilter;
  sort: TxSortOption;
}

interface TxFiltersProps {
  filters: TxFiltersState;
  onChange: (filters: TxFiltersState) => void;
}

const TYPE_OPTIONS: Array<{ value: TxTypeFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "staking", label: "Staking" },
  { value: "settlement", label: "Settlement" },
  { value: "inference_payment", label: "Payment" },
  { value: "reward", label: "Reward" },
];

const STATUS_OPTIONS: Array<{ value: TxStatusFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "confirmed", label: "Confirmed" },
  { value: "pending", label: "Pending" },
  { value: "failed", label: "Failed" },
];

const SORT_OPTIONS: Array<{ value: TxSortOption; label: string }> = [
  { value: "newest", label: "Newest first" },
  { value: "oldest", label: "Oldest first" },
  { value: "amount_high", label: "Amount high-low" },
  { value: "amount_low", label: "Amount low-high" },
];

function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
}: {
  options: Array<{ value: T; label: string }>;
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="inline-flex rounded-lg border border-surface-200 bg-surface-50/50 p-0.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          onClick={() => onChange(opt.value)}
          className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
            value === opt.value
              ? "bg-surface-0 text-surface-900 shadow-sm border border-surface-200"
              : "text-surface-800/50 hover:text-surface-800/70"
          }`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

export function TxFilters({ filters, onChange }: TxFiltersProps) {
  const update = useCallback(
    (partial: Partial<TxFiltersState>) => {
      onChange({ ...filters, ...partial });
    },
    [filters, onChange],
  );

  const activeCount = [
    filters.type !== "all",
    filters.status !== "all",
    filters.sort !== "newest",
  ].filter(Boolean).length;

  const clearAll = () => {
    onChange({ type: "all", status: "all", sort: "newest" });
  };

  return (
    <div className="flex flex-col sm:flex-row sm:items-center gap-3 mb-4">
      {/* Type filter */}
      <div className="flex items-center gap-2">
        <span className="text-xs text-surface-800/40 font-medium uppercase tracking-wide shrink-0">
          Type
        </span>
        <SegmentedControl
          options={TYPE_OPTIONS}
          value={filters.type}
          onChange={(v) => update({ type: v })}
        />
      </div>

      {/* Status filter */}
      <div className="flex items-center gap-2">
        <span className="text-xs text-surface-800/40 font-medium uppercase tracking-wide shrink-0">
          Status
        </span>
        <SegmentedControl
          options={STATUS_OPTIONS}
          value={filters.status}
          onChange={(v) => update({ status: v })}
        />
      </div>

      {/* Sort */}
      <div className="flex items-center gap-2 sm:ml-auto">
        <span className="text-xs text-surface-800/40 font-medium uppercase tracking-wide shrink-0">
          Sort
        </span>
        <select
          value={filters.sort}
          onChange={(e) => update({ sort: e.target.value as TxSortOption })}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-1.5 text-xs font-medium text-surface-800/70 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
        >
          {SORT_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Active filter indicators */}
      {activeCount > 0 && (
        <button
          onClick={clearAll}
          className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium bg-brand-500/10 text-brand-600 hover:bg-brand-500/20 transition-colors shrink-0"
        >
          <span>{activeCount} active</span>
          <span className="text-brand-600/70">&times;</span>
        </button>
      )}
    </div>
  );
}
