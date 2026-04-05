"use client";

import { useState, useMemo } from "react";
import type { AiPointsData } from "@/lib/api/provider";
import { generateHeatmapGrid, colorScaleGreen } from "@/lib/utils/charts";

interface RequestHeatmapProps {
  aiPoints: AiPointsData | null;
}

const WEEKS = 12;
const DAYS_PER_WEEK = 7;
const CELL_SIZE = 12;
const CELL_GAP = 3;

const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/**
 * Generate simulated daily request counts from aiPoints data.
 * Since we don't have per-day granularity, we create a plausible distribution.
 */
function generateSimulatedGrid(aiPoints: AiPointsData | null): number[][] {
  // Grid: rows = days of week (Mon=0..Sun=6), cols = weeks (0..11)
  const grid: number[][] = [];
  for (let d = 0; d < DAYS_PER_WEEK; d++) {
    grid.push(new Array(WEEKS).fill(0));
  }

  if (!aiPoints || aiPoints.totalTokens === 0) return grid;

  // Estimate total requests from tokens (rough: ~500 tokens/request)
  const estimatedRequests = Math.round(aiPoints.totalTokens / 500);

  // Distribute across 84 days (12 weeks * 7 days) with some randomness
  // Use a deterministic seed based on totalTokens for consistency
  const seed = aiPoints.totalTokens;
  let remaining = estimatedRequests;

  for (let week = 0; week < WEEKS; week++) {
    for (let day = 0; day < DAYS_PER_WEEK; day++) {
      // Weighted distribution: weekdays higher than weekends
      const dayWeight = day < 5 ? 1.0 + (Math.sin(seed + week * 3 + day * 7) * 0.3) : 0.6;
      // Recency bias: more recent weeks have more activity
      const weekWeight = 0.5 + (week / WEEKS) * 1.2;
      const base = (remaining / ((WEEKS - week) * DAYS_PER_WEEK - day)) * dayWeight * weekWeight;

      // Deterministic pseudo-random variation
      const variation = 0.3 + Math.abs(Math.sin(seed * 0.001 + week * 13.7 + day * 7.3)) * 1.4;
      const count = Math.max(0, Math.round(base * variation));

      grid[day][week] = Math.min(count, remaining);
      remaining -= count;
      if (remaining <= 0) break;
    }
    if (remaining <= 0) break;
  }

  return grid;
}

/**
 * Get the date string for a given grid position.
 * grid[dayRow][weekCol] where weekCol=0 is the earliest week.
 */
function getDateForCell(dayRow: number, weekCol: number): string {
  const now = new Date();
  // Find the Monday of the earliest week (12 weeks ago)
  const currentDow = now.getDay(); // 0=Sun, 1=Mon...
  const mondayOffset = currentDow === 0 ? -6 : 1 - currentDow;
  const thisMonday = new Date(now);
  thisMonday.setDate(now.getDate() + mondayOffset);

  const targetMonday = new Date(thisMonday);
  targetMonday.setDate(thisMonday.getDate() - (WEEKS - 1 - weekCol) * 7);

  const cellDate = new Date(targetMonday);
  cellDate.setDate(targetMonday.getDate() + dayRow);

  return cellDate.toISOString().slice(0, 10);
}

export function RequestHeatmap({ aiPoints }: RequestHeatmapProps) {
  const [tooltip, setTooltip] = useState<{ value: number; date: string; x: number; y: number } | null>(null);

  const grid = useMemo(() => generateSimulatedGrid(aiPoints), [aiPoints]);
  const cells = useMemo(
    () => generateHeatmapGrid(grid, colorScaleGreen, CELL_SIZE, CELL_GAP),
    [grid],
  );

  const hasData = aiPoints !== null && aiPoints.totalTokens > 0;

  const totalWidth = WEEKS * (CELL_SIZE + CELL_GAP) - CELL_GAP;
  const totalHeight = DAYS_PER_WEEK * (CELL_SIZE + CELL_GAP) - CELL_GAP;
  const leftMargin = 32; // space for day labels
  const topMargin = 4;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <h2 className="font-semibold mb-4 flex items-center gap-2">
        <span className="text-lg">&#x1F4C5;</span> Request Activity
      </h2>

      {!hasData ? (
        <div className="flex items-center justify-center py-10 text-surface-800/40 text-sm">
          <div className="text-center">
            <div className="text-3xl mb-2 opacity-30">&#x1F50C;</div>
            <p>Connect to agent for live data</p>
            <p className="text-xs mt-1">Request activity heatmap will show here once inference starts.</p>
          </div>
        </div>
      ) : (
        <>
          {/* Month labels */}
          <div
            className="flex text-[9px] text-surface-800/40 mb-1"
            style={{ marginLeft: leftMargin }}
          >
            {Array.from({ length: WEEKS }, (_, i) => {
              const d = new Date();
              d.setDate(d.getDate() - (WEEKS - 1 - i) * 7);
              // Only show month label when it changes
              const prev = new Date(d);
              prev.setDate(prev.getDate() - 7);
              if (d.getMonth() === prev.getMonth() && i > 0) {
                return <div key={i} className="flex-1" />;
              }
              return (
                <div key={i} className="flex-1 truncate">
                  {d.toLocaleDateString("en-US", { month: "short" })}
                </div>
              );
            })}
          </div>

          <div className="relative overflow-x-auto">
            {/* Day labels */}
            <div className="absolute top-0 left-0" style={{ width: leftMargin }}>
              {DAY_LABELS.map((label, i) => (
                <div
                  key={label}
                  className="text-[9px] text-surface-800/40 leading-none"
                  style={{
                    height: CELL_SIZE + CELL_GAP,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "flex-end",
                    paddingRight: 6,
                  }}
                >
                  {i % 2 === 0 ? label : ""}
                </div>
              ))}
            </div>

            <div style={{ marginLeft: leftMargin }} className="relative">
              <svg
                width={totalWidth}
                height={totalHeight}
                className="block"
              >
                {cells.map((cell, i) => (
                  <rect
                    key={i}
                    x={cell.x}
                    y={cell.y}
                    width={CELL_SIZE}
                    height={CELL_SIZE}
                    rx={2}
                    fill={cell.color}
                    className="cursor-pointer transition-opacity hover:opacity-80"
                    onMouseEnter={(e) => {
                      const rect = (e.target as SVGElement).getBoundingClientRect();
                      const parentRect = (e.target as SVGElement).closest(".relative")?.getBoundingClientRect();
                      const dayRow = Math.floor(i / WEEKS);
                      const weekCol = i % WEEKS;
                      setTooltip({
                        value: cell.value,
                        date: getDateForCell(dayRow, weekCol),
                        x: rect.left - (parentRect?.left ?? 0) + rect.width / 2,
                        y: rect.top - (parentRect?.top ?? 0) - 8,
                      });
                    }}
                    onMouseLeave={() => setTooltip(null)}
                  />
                ))}
              </svg>

              {/* Tooltip */}
              {tooltip && (
                <div
                  className="absolute z-10 px-2.5 py-1.5 rounded-md bg-surface-900 text-white text-[11px] shadow-lg pointer-events-none whitespace-nowrap -translate-x-1/2 -translate-y-full"
                  style={{ left: tooltip.x, top: tooltip.y }}
                >
                  <span className="font-medium">{tooltip.value}</span>{" "}
                  <span className="text-surface-300">
                    request{tooltip.value !== 1 ? "s" : ""} on {tooltip.date}
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* Legend */}
          <div className="flex items-center justify-end gap-1.5 mt-3 text-[10px] text-surface-800/40">
            <span>Less</span>
            {[0, 0.2, 0.4, 0.6, 0.8].map((t, i) => (
              <span
                key={i}
                className="inline-block w-3 h-3 rounded-sm"
                style={{ backgroundColor: `rgba(34,197,94,${0.15 + t * 0.85})` }}
              />
            ))}
            <span>More</span>
          </div>

          <p className="text-[10px] text-surface-800/30 mt-2">
            Estimated from total token throughput. Connect to agent for precise per-day counts.
          </p>
        </>
      )}
    </div>
  );
}
