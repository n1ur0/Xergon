"use client";

import { cn } from "@/lib/utils";
import { REGIONS, GPU_TYPES, type GpuFilters } from "@/lib/api/gpu";
import { Search, SlidersHorizontal, X, ChevronDown } from "lucide-react";
import { useState } from "react";

interface GpuFiltersBarProps {
  filters: GpuFilters;
  onFiltersChange: (filters: GpuFilters) => void;
  totalResults: number;
}

export function GpuFiltersBar({ filters, onFiltersChange, totalResults }: GpuFiltersBarProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="sticky top-14 z-40 rounded-xl border border-surface-200 bg-surface-0/95 backdrop-blur-sm p-3 sm:p-4 shadow-sm sm:shadow-none sm:static sm:z-auto sm:rounded-xl sm:bg-surface-0 sm:backdrop-blur-none sm:shadow-none">
      {/* Top row: search + filters toggle */}
      <div className="flex flex-col gap-3">
        {/* GPU type search */}
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/40" />
          <input
            type="text"
            placeholder="Search GPU type..."
            value={filters.gpu_type ?? ""}
            onChange={(e) =>
              onFiltersChange({
                ...filters,
                gpu_type: e.target.value || undefined,
              })
            }
            className="w-full rounded-lg border border-surface-200 bg-surface-50 pl-9 pr-10 py-2.5 sm:py-2 text-sm text-surface-900 placeholder:text-surface-800/40 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 transition-colors min-h-[44px] sm:min-h-0"
          />
          {/* Clear button */}
          {filters.gpu_type && (
            <button
              onClick={() => onFiltersChange({ ...filters, gpu_type: undefined })}
              className="absolute right-3 top-1/2 -translate-y-1/2 p-0.5 rounded text-surface-800/40 hover:text-surface-800/70"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>

        {/* Quick region pills - horizontal scroll on mobile */}
        <div className="flex items-center gap-2 overflow-x-auto scrollbar-none pb-1 sm:flex-wrap sm:overflow-x-visible sm:pb-0">
          <button
            onClick={() =>
              onFiltersChange({ ...filters, region: undefined })
            }
            className={cn(
              "rounded-full px-3 py-1.5 sm:py-1 text-xs font-medium transition-colors whitespace-nowrap min-h-[36px] sm:min-h-0 flex-shrink-0",
              !filters.region
                ? "bg-surface-900 text-white"
                : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
            )}
          >
            All Regions
          </button>
          {REGIONS.map((r) => (
            <button
              key={r}
              onClick={() =>
                onFiltersChange({
                  ...filters,
                  region: filters.region === r ? undefined : r,
                })
              }
              className={cn(
                "rounded-full px-3 py-1.5 sm:py-1 text-xs font-medium transition-colors whitespace-nowrap min-h-[36px] sm:min-h-0 flex-shrink-0",
                filters.region === r
                  ? "bg-brand-600 text-white"
                  : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
              )}
            >
              {r}
            </button>
          ))}

          {/* Expand toggle - right aligned on mobile */}
          <button
            onClick={() => setExpanded(!expanded)}
            className={cn(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 sm:py-2 text-xs sm:text-sm font-medium transition-colors whitespace-nowrap min-h-[36px] sm:min-h-0 flex-shrink-0 ml-auto",
              expanded
                ? "bg-brand-100 text-brand-700"
                : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
            )}
          >
            <SlidersHorizontal className="w-3.5 h-3.5 sm:w-4 sm:h-4" />
            <span className="hidden sm:inline">Filters</span>
            <ChevronDown
              className={cn(
                "w-3.5 h-3.5 transition-transform sm:ml-0.5",
                expanded && "rotate-180",
              )}
            />
          </button>
        </div>
      </div>

      {/* Expanded filters */}
      <div
        className={cn(
          "grid transition-all duration-200 ease-in-out overflow-hidden",
          expanded ? "grid-rows-[1fr] mt-3 pt-3 border-t border-surface-100 opacity-100" : "grid-rows-[0fr] opacity-0",
        )}
      >
        <div className="overflow-hidden">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {/* Min VRAM slider */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">
                Min VRAM: {filters.min_vram ?? 0} GB
              </label>
              <input
                type="range"
                min={0}
                max={96}
                step={4}
                value={filters.min_vram ?? 0}
                onChange={(e) =>
                  onFiltersChange({
                    ...filters,
                    min_vram: Number(e.target.value) || undefined,
                  })
                }
                className="w-full accent-brand-600"
              />
              <div className="flex justify-between text-xs text-surface-800/40 mt-0.5">
                <span>0 GB</span>
                <span>96 GB</span>
              </div>
            </div>

            {/* Max Price input */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">
                Max Price per Hour (ERG)
              </label>
              <input
                type="number"
                step="0.01"
                min={0}
                placeholder="No limit"
                value={filters.max_price ?? ""}
                onChange={(e) =>
                  onFiltersChange({
                    ...filters,
                    max_price: e.target.value ? Number(e.target.value) : undefined,
                  })
                }
                className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2.5 sm:py-2 text-sm text-surface-900 placeholder:text-surface-800/40 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 transition-colors min-h-[44px] sm:min-h-0"
              />
            </div>

            {/* GPU type quick picks */}
            <div className="sm:col-span-2">
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">
                GPU Type
              </label>
              <div className="flex flex-wrap gap-1.5">
                {GPU_TYPES.map((g) => (
                  <button
                    key={g}
                    onClick={() =>
                      onFiltersChange({
                        ...filters,
                        gpu_type: filters.gpu_type === g ? undefined : g,
                      })
                    }
                    className={cn(
                      "rounded-full px-2.5 py-1.5 sm:py-1 text-xs font-medium transition-colors min-h-[36px] sm:min-h-0",
                      filters.gpu_type === g
                        ? "bg-brand-600 text-white"
                        : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
                    )}
                  >
                    {g}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Results count */}
      <div className="mt-2 sm:mt-3 text-xs text-surface-800/40">
        {totalResults} listing{totalResults !== 1 ? "s" : ""} found
      </div>
    </div>
  );
}
