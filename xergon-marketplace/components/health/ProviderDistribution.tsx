"use client";

import { useEffect, useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProviderDistributionProps {
  online: number;
  degraded: number;
  offline: number;
  total: number;
}

// ---------------------------------------------------------------------------
// SVG ring chart helpers
// ---------------------------------------------------------------------------

const SIZE = 120;
const STROKE_WIDTH = 14;
const RADIUS = (SIZE - STROKE_WIDTH) / 2;
const CENTER = SIZE / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

interface Segment {
  value: number;
  color: string;
  label: string;
}

function computeSegments(
  online: number,
  degraded: number,
  offline: number,
): Segment[] {
  const segments: Segment[] = [];
  if (online > 0) segments.push({ value: online, color: "#10b981", label: "Online" });
  if (degraded > 0) segments.push({ value: degraded, color: "#f59e0b", label: "Degraded" });
  if (offline > 0) segments.push({ value: offline, color: "#ef4444", label: "Offline" });
  // If all zero, show a grey placeholder
  if (segments.length === 0) {
    segments.push({ value: 1, color: "#d1d5db", label: "No data" });
  }
  return segments;
}

function segmentArcs(segments: Segment[]): Array<{ offset: number; length: number; color: string }> {
  const total = segments.reduce((s, seg) => s + seg.value, 0);
  const arcs: Array<{ offset: number; length: number; color: string }> = [];
  let currentOffset = 0;

  for (const seg of segments) {
    const fraction = seg.value / total;
    const length = fraction * CIRCUMFERENCE;
    // Small gap between segments
    const gap = segments.length > 1 ? 2 : 0;
    arcs.push({
      offset: currentOffset,
      length: Math.max(0, length - gap),
      color: seg.color,
    });
    currentOffset += length;
  }

  return arcs;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProviderDistribution({
  online,
  degraded,
  offline,
  total,
}: ProviderDistributionProps) {
  const [animated, setAnimated] = useState(false);
  const segments = computeSegments(online, degraded, offline);
  const arcs = segmentArcs(segments);

  useEffect(() => {
    const timer = requestAnimationFrame(() => setAnimated(true));
    return () => cancelAnimationFrame(timer);
  }, []);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 flex flex-col items-center">
      <h2 className="text-base font-semibold text-surface-900 mb-4">
        Provider Network
      </h2>

      {/* SVG Ring */}
      <div className="relative" style={{ width: SIZE, height: SIZE }}>
        <svg
          width={SIZE}
          height={SIZE}
          viewBox={`0 0 ${SIZE} ${SIZE}`}
          role="img"
          aria-label={`Provider distribution: ${online} online, ${degraded} degraded, ${offline} offline`}
        >
          {/* Background ring */}
          <circle
            stroke="currentColor"
            strokeOpacity={0.08}
            fill="transparent"
            strokeWidth={STROKE_WIDTH}
            r={RADIUS}
            cx={CENTER}
            cy={CENTER}
            className="text-surface-800"
          />

          {/* Colored segments */}
          {arcs.map((arc, i) => (
            <circle
              key={i}
              stroke={arc.color}
              fill="transparent"
              strokeWidth={STROKE_WIDTH}
              strokeLinecap="round"
              strokeDasharray={`${animated ? arc.length : 0} ${CIRCUMFERENCE}`}
              strokeDashoffset={-arc.offset}
              r={RADIUS}
              cx={CENTER}
              cy={CENTER}
              transform={`rotate(-90 ${CENTER} ${CENTER})`}
              style={{
                transition: "stroke-dasharray 0.8s ease-out",
                transitionDelay: `${i * 0.1}s`,
              }}
            />
          ))}
        </svg>

        {/* Center text */}
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="text-2xl font-bold text-surface-900">{total}</span>
          <span className="text-[10px] text-surface-800/40">providers</span>
        </div>
      </div>

      {/* Legend */}
      <div className="flex items-center gap-4 mt-4">
        {segments.map((seg) => (
          <div key={seg.label} className="flex items-center gap-1.5">
            <span
              className="h-2.5 w-2.5 rounded-full"
              style={{ backgroundColor: seg.color }}
            />
            <span className="text-xs text-surface-800/60">
              {seg.label}{" "}
              <span className="font-medium text-surface-900">{seg.value}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
