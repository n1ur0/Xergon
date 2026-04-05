"use client";

import { useState, useMemo } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelEntry {
  model: string;
  requests: number;
  tokens: number;
}

interface TopModelsTableProps {
  models: ModelEntry[];
}

type SortKey = "requests" | "tokens" | "model";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function TopModelsTable({ models }: TopModelsTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>("requests");
  const [sortAsc, setSortAsc] = useState(false);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortAsc((prev) => !prev);
    } else {
      setSortKey(key);
      setSortAsc(false);
    }
  };

  const sortedModels = useMemo(() => {
    return [...models].sort((a, b) => {
      const cmp =
        sortKey === "model"
          ? a.model.localeCompare(b.model)
          : a[sortKey] - b[sortKey];
      return sortAsc ? cmp : -cmp;
    });
  }, [models, sortKey, sortAsc]);

  const totalRequests = useMemo(
    () => models.reduce((s, m) => s + m.requests, 0),
    [models],
  );

  if (models.length === 0) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 text-center text-surface-800/50 text-sm">
        No model data available.
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
      <div className="px-5 py-4 border-b border-surface-100">
        <h2 className="text-base font-semibold text-surface-900">
          Top Models
        </h2>
        <p className="text-xs text-surface-800/40 mt-0.5">
          Ranked by usage across the network
        </p>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-surface-100">
              <th scope="col" className="px-5 py-3 text-left text-xs font-medium text-surface-800/50 w-12">
                #
              </th>
              <th
                scope="col"
                className={cn(
                  "px-5 py-3 text-left text-xs font-medium text-surface-800/50 cursor-pointer hover:text-surface-800/70 select-none",
                )}
              >
                <button
                  type="button"
                  onClick={() => handleSort("model")}
                  className="flex items-center gap-1 w-full text-left"
                  aria-sort={sortKey === "model" ? (sortAsc ? "ascending" : "descending") : "none"}
                >
                  Model
                  {sortKey === "model" && (
                    <span className="ml-1" aria-hidden="true">{sortAsc ? "↑" : "↓"}</span>
                  )}
                </button>
              </th>
              <th
                scope="col"
                className="px-5 py-3 text-right text-xs font-medium text-surface-800/50"
              >
                <button
                  type="button"
                  onClick={() => handleSort("requests")}
                  className="flex items-center justify-end gap-1 w-full text-right"
                  aria-sort={sortKey === "requests" ? (sortAsc ? "ascending" : "descending") : "none"}
                >
                  Requests
                  {sortKey === "requests" && (
                    <span className="ml-1" aria-hidden="true">{sortAsc ? "↑" : "↓"}</span>
                  )}
                </button>
              </th>
              <th
                scope="col"
                className="px-5 py-3 text-right text-xs font-medium text-surface-800/50"
              >
                <button
                  type="button"
                  onClick={() => handleSort("tokens")}
                  className="flex items-center justify-end gap-1 w-full text-right"
                  aria-sort={sortKey === "tokens" ? (sortAsc ? "ascending" : "descending") : "none"}
                >
                  Tokens
                  {sortKey === "tokens" && (
                    <span className="ml-1" aria-hidden="true">{sortAsc ? "↑" : "↓"}</span>
                  )}
                </button>
              </th>
              <th scope="col" className="px-5 py-3 text-left text-xs font-medium text-surface-800/50 w-48">
                Share
              </th>
            </tr>
          </thead>
          <tbody>
            {sortedModels.map((model, i) => {
              const share =
                totalRequests > 0
                  ? ((model.requests / totalRequests) * 100).toFixed(1)
                  : "0";
              return (
                <tr
                  key={model.model}
                  className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors"
                >
                  <td className="px-5 py-3 text-surface-800/30 font-mono text-xs">
                    {i + 1}
                  </td>
                  <td className="px-5 py-3">
                    <span className="font-medium text-surface-900">
                      {model.model}
                    </span>
                  </td>
                  <td className="px-5 py-3 text-right font-mono text-surface-800/70">
                    {formatNumber(model.requests)}
                  </td>
                  <td className="px-5 py-3 text-right font-mono text-surface-800/70">
                    {formatNumber(model.tokens)}
                  </td>
                  <td className="px-5 py-3">
                    <div className="flex items-center gap-2">
                      <div
                        className="flex-1 h-1.5 rounded-full bg-surface-100 overflow-hidden"
                        role="progressbar"
                        aria-valuenow={parseFloat(share)}
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-label={`${model.model}: ${share}% share of requests`}
                      >
                        <div
                          className="h-full rounded-full bg-brand-500 transition-all duration-500"
                          style={{
                            width: `${Math.max(
                              parseFloat(share),
                              0.5,
                            )}%`,
                          }}
                        />
                      </div>
                      <span className="text-xs text-surface-800/40 w-10 text-right">
                        {share}%
                      </span>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
