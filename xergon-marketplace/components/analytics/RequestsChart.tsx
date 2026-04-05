"use client";

import { useState, useMemo, useCallback, useRef } from "react";
import {
  generateLinePath,
  generateAreaPath,
  formatAxisValue,
  calculateYAxisTicks,
  type ChartPadding,
} from "@/lib/utils/charts";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface DataPoint {
  timestamp: string;
  count: number;
}

interface RequestsChartProps {
  data: DataPoint[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const CHART_WIDTH = 700;
const CHART_HEIGHT = 260;
const PADDING: ChartPadding = { top: 20, right: 20, bottom: 40, left: 55 };

function formatTimeLabel(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatDateLabel(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

function Tooltip({
  point,
  x,
  y,
}: {
  point: DataPoint;
  x: number;
  y: number;
}) {
  return (
    <g>
      <rect
        x={x - 52}
        y={y - 44}
        width={104}
        height={36}
        rx={6}
        className="fill-surface-900 stroke-surface-700"
        strokeWidth={1}
      />
      <text
        x={x}
        y={y - 26}
        textAnchor="middle"
        className="fill-surface-0 text-[10px] font-medium"
      >
        {point.count.toLocaleString()} reqs
      </text>
      <text
        x={x}
        y={y - 14}
        textAnchor="middle"
        className="fill-surface-0/60 text-[9px]"
      >
        {formatTimeLabel(point.timestamp)}
      </text>
    </g>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function RequestsChart({ data }: RequestsChartProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [range, setRange] = useState<"24h" | "7d">("24h");
  const svgRef = useRef<SVGSVGElement>(null);

  // Filter data by range
  const filteredData = useMemo(() => {
    if (!data || data.length === 0) return [];
    if (range === "24h") return data.slice(-24);
    return data.slice(-168); // 7 days * 24 hours
  }, [data, range]);

  const values = useMemo(() => filteredData.map((d) => d.count), [filteredData]);
  const maxVal = useMemo(() => Math.max(...values, 1), [values]);
  const minVal = useMemo(() => Math.min(...values, 0), [values]);

  const linePath = useMemo(
    () => generateLinePath(values, CHART_WIDTH, CHART_HEIGHT, PADDING),
    [values],
  );
  const areaPath = useMemo(
    () => generateAreaPath(values, CHART_WIDTH, CHART_HEIGHT, PADDING),
    [values],
  );

  const yTicks = useMemo(
    () => calculateYAxisTicks(minVal, maxVal, 5),
    [minVal, maxVal],
  );

  // Compute SVG data point positions for hover
  const pointPositions = useMemo(() => {
    if (values.length === 0) return [];
    const plotW = CHART_WIDTH - PADDING.left - PADDING.right;
    const plotH = CHART_HEIGHT - PADDING.top - PADDING.bottom;
    const range = maxVal - minVal || 1;

    return values.map((v, i) => ({
      x:
        PADDING.left +
        (values.length > 1 ? (i / (values.length - 1)) * plotW : plotW / 2),
      y: PADDING.top + plotH - ((v - minVal) / range) * plotH,
    }));
  }, [values, maxVal, minVal]);

  // X-axis labels (show every Nth label to avoid crowding)
  const xLabels = useMemo(() => {
    const step = Math.max(1, Math.floor(filteredData.length / 6));
    return filteredData
      .map((d, i) => ({
        label: range === "24h" ? formatTimeLabel(d.timestamp) : formatDateLabel(d.timestamp),
        index: i,
      }))
      .filter((_, i) => i % step === 0);
  }, [filteredData, range]);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      if (!svgRef.current || pointPositions.length === 0) return;
      const rect = svgRef.current.getBoundingClientRect();
      const scaleX = CHART_WIDTH / rect.width;
      const mouseX = (e.clientX - rect.left) * scaleX;

      // Find closest point
      let closestIdx = 0;
      let closestDist = Infinity;
      for (let i = 0; i < pointPositions.length; i++) {
        const dist = Math.abs(pointPositions[i].x - mouseX);
        if (dist < closestDist) {
          closestDist = dist;
          closestIdx = i;
        }
      }
      setHoveredIndex(closestIdx);
    },
    [pointPositions],
  );

  const handleMouseLeave = useCallback(() => setHoveredIndex(null), []);

  if (filteredData.length === 0) {
    return (
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 text-center text-surface-800/50 text-sm">
        No request data available.
      </div>
    );
  }

  // Build a descriptive aria-label for screen readers
  const chartAriaLabel = useMemo(() => {
    const rangeLabel = range === "24h" ? "last 24 hours" : "last 7 days";
    const totalReqs = filteredData.reduce((sum, d) => sum + d.count, 0);
    const maxCount = Math.max(...filteredData.map((d) => d.count), 0);
    const minCount = Math.min(...filteredData.map((d) => d.count), 0);
    return `Requests over time chart for the ${rangeLabel}. ` +
      `${filteredData.length} data points. ` +
      `Total requests: ${totalReqs.toLocaleString()}. ` +
      `Peak: ${maxCount.toLocaleString()} requests. ` +
      `Lowest: ${minCount.toLocaleString()} requests.`;
  }, [filteredData, range]);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-base font-semibold text-surface-900">
          Requests Over Time
        </h2>
        <div className="flex gap-1">
          {(["24h", "7d"] as const).map((r) => (
            <button
              key={r}
              onClick={() => setRange(r)}
              className={`rounded-lg px-3 py-1 text-xs font-medium transition-colors ${
                range === r
                  ? "bg-surface-900 text-white"
                  : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"
              }`}
            >
              {r.toUpperCase()}
            </button>
          ))}
        </div>
      </div>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
        className="w-full h-auto"
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
        role="img"
        aria-label={chartAriaLabel}
      >
        <defs>
          <linearGradient id="areaGradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgba(51,120,255,0.25)" />
            <stop offset="100%" stopColor="rgba(51,120,255,0.02)" />
          </linearGradient>
        </defs>

        {/* Grid lines */}
        {yTicks.map((tick) => {
          const plotH = CHART_HEIGHT - PADDING.top - PADDING.bottom;
          const range = maxVal - minVal || 1;
          const y =
            PADDING.top + plotH - ((tick - minVal) / range) * plotH;
          return (
            <line
              key={tick}
              x1={PADDING.left}
              y1={y}
              x2={CHART_WIDTH - PADDING.right}
              y2={y}
              className="stroke-surface-200"
              strokeWidth={0.5}
            />
          );
        })}

        {/* Y-axis labels */}
        {yTicks.map((tick) => {
          const plotH = CHART_HEIGHT - PADDING.top - PADDING.bottom;
          const range = maxVal - minVal || 1;
          const y =
            PADDING.top + plotH - ((tick - minVal) / range) * plotH;
          return (
            <text
              key={tick}
              x={PADDING.left - 8}
              y={y + 3}
              textAnchor="end"
              className="fill-surface-800/40 text-[10px]"
            >
              {formatAxisValue(tick)}
            </text>
          );
        })}

        {/* X-axis labels */}
        {xLabels.map((item) => {
          const plotW = CHART_WIDTH - PADDING.left - PADDING.right;
          const x =
            PADDING.left +
            (filteredData.length > 1
              ? (item.index / (filteredData.length - 1)) * plotW
              : plotW / 2);
          return (
            <text
              key={item.index}
              x={x}
              y={CHART_HEIGHT - 8}
              textAnchor="middle"
              className="fill-surface-800/40 text-[10px]"
            >
              {item.label}
            </text>
          );
        })}

        {/* Area fill */}
        <path d={areaPath} fill="url(#areaGradient)" />

        {/* Line */}
        <polyline
          points={linePath}
          fill="none"
          className="stroke-brand-500"
          strokeWidth={2}
          strokeLinejoin="round"
          strokeLinecap="round"
        />

        {/* Hover crosshair line */}
        {hoveredIndex !== null && pointPositions[hoveredIndex] && (
          <line
            x1={pointPositions[hoveredIndex].x}
            y1={PADDING.top}
            x2={pointPositions[hoveredIndex].x}
            y2={CHART_HEIGHT - PADDING.bottom}
            className="stroke-brand-400"
            strokeWidth={0.5}
            strokeDasharray="4 3"
          />
        )}

        {/* Data points */}
        {hoveredIndex !== null && pointPositions[hoveredIndex] && (
          <>
            <circle
              cx={pointPositions[hoveredIndex].x}
              cy={pointPositions[hoveredIndex].y}
              r={4}
              className="fill-brand-500"
            />
            <circle
              cx={pointPositions[hoveredIndex].x}
              cy={pointPositions[hoveredIndex].y}
              r={8}
              className="fill-brand-500/20"
            />
            <Tooltip
              point={filteredData[hoveredIndex]}
              x={pointPositions[hoveredIndex].x}
              y={pointPositions[hoveredIndex].y}
            />
          </>
        )}
      </svg>
    </div>
  );
}
