"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderFilters } from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface ProviderFiltersBarProps {
  filters: ProviderFilters;
  onChange: (filters: ProviderFilters) => void;
  availableModels: string[];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProviderFiltersBar({
  filters,
  onChange,
  availableModels,
}: ProviderFiltersBarProps) {
  const [localSearch, setLocalSearch] = useState(filters.search);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Debounced search input
  const handleSearchChange = useCallback(
    (value: string) => {
      setLocalSearch(value);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        onChange({ ...filters, search: value });
      }, 300);
    },
    [filters, onChange],
  );

  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  const handleSelectChange = useCallback(
    (key: keyof ProviderFilters, value: string) => {
      onChange({ ...filters, [key]: value });
    },
    [filters, onChange],
  );

  const handleToggleOrder = useCallback(() => {
    onChange({
      ...filters,
      sortOrder: filters.sortOrder === "asc" ? "desc" : "asc",
    });
  }, [filters, onChange]);

  const handleClearFilters = useCallback(() => {
    setLocalSearch("");
    onChange({
      search: "",
      region: "all",
      status: "all",
      model: "all",
      sortBy: "aiPoints",
      sortOrder: "desc",
    });
  }, [onChange]);

  // Count active filters
  const activeCount =
    (filters.search ? 1 : 0) +
    (filters.region !== "all" ? 1 : 0) +
    (filters.status !== "all" ? 1 : 0) +
    (filters.model !== "all" ? 1 : 0);

  const selectClasses =
    "h-9 rounded-lg border border-surface-200 bg-surface-0 px-3 text-sm text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500";

  return (
    <div className="space-y-3">
      {/* Search bar */}
      <div className="relative">
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-surface-800/40"
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          id="provider-search"
          type="text"
          value={localSearch}
          onChange={(e) => handleSearchChange(e.target.value)}
          placeholder="Search providers, models, GPUs..."
          aria-label="Search providers"
          className="w-full h-10 pl-9 pr-4 rounded-lg border border-surface-200 bg-surface-0 text-sm text-surface-900 placeholder:text-surface-800/40 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
        />
      </div>

      {/* Filter row */}
      <div className="flex flex-wrap items-center gap-2">
        {/* Region */}
        <select
          value={filters.region}
          onChange={(e) => handleSelectChange("region", e.target.value)}
          className={selectClasses}
          aria-label="Filter by region"
        >
          <option value="all">All Regions</option>
          <option value="US">🇺🇸 US</option>
          <option value="EU">🇪🇺 EU</option>
          <option value="Asia">🌏 Asia</option>
          <option value="Other">🌍 Other</option>
        </select>

        {/* Status */}
        <select
          value={filters.status}
          onChange={(e) => handleSelectChange("status", e.target.value)}
          className={selectClasses}
          aria-label="Filter by status"
        >
          <option value="all">All Status</option>
          <option value="online">Online</option>
          <option value="degraded">Degraded</option>
          <option value="offline">Offline</option>
        </select>

        {/* Model */}
        <select
          value={filters.model}
          onChange={(e) => handleSelectChange("model", e.target.value)}
          className={selectClasses}
          aria-label="Filter by model"
        >
          <option value="all">All Models</option>
          {availableModels.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>

        {/* Sort by */}
        <select
          value={filters.sortBy}
          onChange={(e) => handleSelectChange("sortBy", e.target.value)}
          className={selectClasses}
          aria-label="Sort by"
        >
          <option value="aiPoints">AI Points</option>
          <option value="uptime">Uptime</option>
          <option value="tokens">Tokens</option>
          <option value="price">Price</option>
          <option value="name">Name</option>
        </select>

        {/* Sort order toggle */}
        <button
          type="button"
          onClick={handleToggleOrder}
          className={`inline-flex items-center justify-center h-9 w-9 rounded-lg border border-surface-200 bg-surface-0 text-surface-800/70 hover:bg-surface-100 transition-colors ${selectClasses}`}
          aria-label={`Sort ${filters.sortOrder === "asc" ? "ascending" : "descending"}`}
          title={filters.sortOrder === "asc" ? "Ascending" : "Descending"}
        >
          {filters.sortOrder === "asc" ? (
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="m3 16 4 4 4-4" /><path d="M7 20V4" />
            </svg>
          ) : (
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="m3 8 4-4 4 4" /><path d="M7 4v16" />
            </svg>
          )}
        </button>

        {/* Spacer */}
        <div className="flex-1" />

        {/* Active filter count + clear */}
        {activeCount > 0 && (
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1 text-xs font-medium px-2 py-1 rounded-full bg-brand-500/10 text-brand-600">
              {activeCount} filter{activeCount !== 1 ? "s" : ""}
            </span>
            <button
              type="button"
              onClick={handleClearFilters}
              className="text-xs font-medium text-surface-800/60 hover:text-surface-900 transition-colors px-2 py-1 rounded-lg hover:bg-surface-100"
            >
              Clear filters
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
