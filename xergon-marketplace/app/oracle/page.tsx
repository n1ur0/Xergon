"use client";

import { useState, useEffect, useCallback } from "react";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OracleOperator {
  name: string;
  lastDatapoint: string;
  epoch: number;
  status: "active" | "stale" | "offline";
  rewardTokens: number;
}

interface EpochDataPoint {
  epoch: number;
  price: number;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_CURRENT = {
  price: 1.24,
  epoch: 4782,
  lastRefresh: "2 min ago",
  reportingOracles: 5,
  totalOracles: 6,
};

const MOCK_OPERATORS: OracleOperator[] = [
  { name: "ergo-oracle-1", lastDatapoint: "$1.24", epoch: 4782, status: "active", rewardTokens: 12.4 },
  { name: "ergo-oracle-2", lastDatapoint: "$1.23", epoch: 4782, status: "active", rewardTokens: 11.8 },
  { name: "ergo-oracle-3", lastDatapoint: "$1.25", epoch: 4782, status: "active", rewardTokens: 13.1 },
  { name: "ergo-oracle-4", lastDatapoint: "$1.24", epoch: 4781, status: "active", rewardTokens: 10.2 },
  { name: "ergo-oracle-5", lastDatapoint: "$1.21", epoch: 4780, status: "stale", rewardTokens: 8.7 },
  { name: "ergo-oracle-6", lastDatapoint: "$1.18", epoch: 4774, status: "offline", rewardTokens: 3.1 },
];

const MOCK_EPOCH_HISTORY: EpochDataPoint[] = [
  { epoch: 4773, price: 1.19 },
  { epoch: 4774, price: 1.20 },
  { epoch: 4775, price: 1.18 },
  { epoch: 4776, price: 1.22 },
  { epoch: 4777, price: 1.21 },
  { epoch: 4778, price: 1.23 },
  { epoch: 4779, price: 1.25 },
  { epoch: 4780, price: 1.24 },
  { epoch: 4781, price: 1.22 },
  { epoch: 4782, price: 1.24 },
];

const ORACLE_PARAMS = {
  epochLength: "12 blocks (~40 min)",
  minDataPoints: 4,
  maxDeviation: "5%",
  dataPointsPerEpoch: 6,
};

// ---------------------------------------------------------------------------
// Skeleton loaders
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function OracleSkeleton() {
  return (
    <div className="space-y-6">
      {/* Hero skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <div className="flex flex-col sm:flex-row sm:items-end gap-4">
          <SkeletonPulse className="h-14 w-36" />
          <div className="space-y-2">
            <SkeletonPulse className="h-4 w-48" />
            <SkeletonPulse className="h-3 w-32" />
          </div>
        </div>
        <div className="mt-4 flex gap-4">
          <SkeletonPulse className="h-8 w-24 rounded-full" />
          <SkeletonPulse className="h-8 w-24 rounded-full" />
        </div>
      </div>
      {/* Table skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-100">
          <SkeletonPulse className="h-5 w-32 mb-1" />
          <SkeletonPulse className="h-3 w-48" />
        </div>
        <div className="space-y-0">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4 px-5 py-3 border-b border-surface-50">
              <SkeletonPulse className="h-4 w-32" />
              <div className="flex-1" />
              <SkeletonPulse className="h-4 w-16" />
              <SkeletonPulse className="h-4 w-16" />
              <SkeletonPulse className="h-5 w-16 rounded-full" />
              <SkeletonPulse className="h-4 w-16" />
            </div>
          ))}
        </div>
      </div>
      {/* Chart skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <SkeletonPulse className="h-5 w-36 mb-4" />
        <SkeletonPulse className="h-[200px] w-full" />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Status dot
// ---------------------------------------------------------------------------

function StatusDot({ status }: { status: OracleOperator["status"] }) {
  const colors: Record<OracleOperator["status"], string> = {
    active: "bg-emerald-500",
    stale: "bg-amber-500",
    offline: "bg-red-500",
  };
  const labels: Record<OracleOperator["status"], string> = {
    active: "Active",
    stale: "Stale",
    offline: "Offline",
  };
  return (
    <span className="inline-flex items-center gap-1.5">
      <span
        className={`h-2 w-2 rounded-full ${colors[status]} ${status === "stale" ? "animate-pulse" : ""}`}
        aria-hidden="true"
      />
      <span className="text-xs text-surface-800/60">{labels[status]}</span>
    </span>
  );
}

// ---------------------------------------------------------------------------
// Inline SVG line chart
// ---------------------------------------------------------------------------

function EpochChart({ data }: { data: EpochDataPoint[] }) {
  const width = 600;
  const height = 200;
  const padX = 50;
  const padY = 20;
  const padBottom = 36;
  const padRight = 16;
  const plotW = width - padX - padRight;
  const plotH = height - padY - padBottom;

  const prices = data.map((d) => d.price);
  const minP = Math.min(...prices) - 0.02;
  const maxP = Math.max(...prices) + 0.02;
  const range = maxP - minP || 1;

  const points = data.map((d, i) => {
    const x = padX + (data.length > 1 ? (i / (data.length - 1)) * plotW : plotW / 2);
    const y = padY + plotH - ((d.price - minP) / range) * plotH;
    return { x, y, ...d };
  });

  const linePath = points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
  const areaPath =
    `M${points[0].x.toFixed(1)},${(padY + plotH).toFixed(1)} ` +
    points.map((p) => `L${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ") +
    ` L${points[points.length - 1].x.toFixed(1)},${(padY + plotH).toFixed(1)} Z`;

  // Y-axis ticks
  const yTicks = [minP, (minP + maxP) / 2, maxP];

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="w-full h-auto"
      role="img"
      aria-label={`ERG/USD price chart over ${data.length} epochs, ranging $${minP.toFixed(2)} to $${maxP.toFixed(2)}`}
    >
      {/* Grid lines */}
      {yTicks.map((tick) => {
        const y = padY + plotH - ((tick - minP) / range) * plotH;
        return (
          <g key={tick}>
            <line
              x1={padX}
              y1={y}
              x2={width - padRight}
              y2={y}
              stroke="currentColor"
              className="text-surface-200 dark:text-surface-700"
              strokeWidth={1}
              strokeDasharray="4 4"
            />
            <text
              x={padX - 8}
              y={y + 4}
              textAnchor="end"
              className="fill-surface-800/40 text-[10px]"
            >
              ${tick.toFixed(2)}
            </text>
          </g>
        );
      })}

      {/* X-axis epoch labels */}
      {points.filter((_, i) => i % 2 === 0).map((p) => (
        <text
          key={p.epoch}
          x={p.x}
          y={height - 6}
          textAnchor="middle"
          className="fill-surface-800/40 text-[10px]"
        >
          {p.epoch}
        </text>
      ))}

      {/* Area fill */}
      <path d={areaPath} className="fill-brand-500/10" />

      {/* Line */}
      <polyline
        points={points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ")}
        fill="none"
        className="stroke-brand-500"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Data points */}
      {points.map((p) => (
        <circle
          key={p.epoch}
          cx={p.x}
          cy={p.y}
          r={3.5}
          className="fill-brand-500 stroke-surface-0"
          strokeWidth={2}
        />
      ))}
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function OraclePage() {
  const [current, setCurrent] = useState(MOCK_CURRENT);
  const [operators, setOperators] = useState<OracleOperator[]>([]);
  const [epochHistory, setEpochHistory] = useState<EpochDataPoint[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  // Simulate data loading
  const loadData = useCallback(() => {
    // In production, this would fetch from the Ergo node API
    // GET /api/v1/boxes/unspent/byTokenId/{oraclePoolTokenId}
    setIsLoading(true);
    setTimeout(() => {
      setCurrent(MOCK_CURRENT);
      setOperators(MOCK_OPERATORS);
      setEpochHistory(MOCK_EPOCH_HISTORY);
      setIsLoading(false);
    }, 800);
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-surface-900">
          ERG Price Oracle
        </h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          EIP-23 compliant decentralized price feed
        </p>
      </div>

      <SuspenseWrap fallback={<OracleSkeleton />}>
        {isLoading ? (
          <OracleSkeleton />
        ) : (
          <>
            {/* Current rate hero card */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
              <div className="flex flex-col sm:flex-row sm:items-end gap-4">
                <div>
                  <p className="text-xs text-surface-800/50 mb-1">ERG / USD</p>
                  <p className="text-5xl font-bold text-surface-900 tracking-tight">
                    ${current.price.toFixed(2)}
                  </p>
                </div>
                <div className="sm:ml-auto flex flex-wrap gap-3">
                  <span className="inline-flex items-center gap-1.5 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/60">
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-surface-800/40" aria-hidden="true">
                      <circle cx="12" cy="12" r="10" />
                      <polyline points="12 6 12 12 16 14" />
                    </svg>
                    Epoch {current.epoch}
                  </span>
                  <span className="inline-flex items-center gap-1.5 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/60">
                    Last refresh: {current.lastRefresh}
                  </span>
                  <span className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium ${
                    current.reportingOracles >= current.totalOracles * 0.8
                      ? "border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 text-emerald-700 dark:text-emerald-400"
                      : "border-amber-200 bg-amber-50 dark:border-amber-800/40 dark:bg-amber-950/20 text-amber-700 dark:text-amber-400"
                  }`}>
                    <span className={`h-1.5 w-1.5 rounded-full ${current.reportingOracles >= current.totalOracles * 0.8 ? "bg-emerald-500" : "bg-amber-500"}`} aria-hidden="true" />
                    {current.reportingOracles} / {current.totalOracles} oracles
                  </span>
                </div>
              </div>
            </div>

            {/* Oracle operators table */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden mb-6">
              <div className="px-5 py-4 border-b border-surface-100">
                <h2 className="text-base font-semibold text-surface-900">
                  Oracle Operators
                </h2>
                <p className="text-xs text-surface-800/40 mt-0.5">
                  Data providers feeding into the oracle pool
                </p>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs text-surface-800/40 border-b border-surface-100">
                      <th className="px-5 py-2.5 font-medium">Oracle</th>
                      <th className="px-3 py-2.5 font-medium">Last Datapoint</th>
                      <th className="px-3 py-2.5 font-medium text-right">Epoch</th>
                      <th className="px-3 py-2.5 font-medium">Status</th>
                      <th className="px-5 py-2.5 font-medium text-right">Reward Tokens</th>
                    </tr>
                  </thead>
                  <tbody>
                    {operators.map((op) => (
                      <tr
                        key={op.name}
                        className="border-b border-surface-50 hover:bg-surface-50 dark:hover:bg-surface-900/30 transition-colors"
                      >
                        <td className="px-5 py-2.5 font-mono text-xs text-surface-900">
                          {op.name}
                        </td>
                        <td className="px-3 py-2.5 text-xs font-semibold text-surface-900">
                          {op.lastDatapoint}
                        </td>
                        <td className="px-3 py-2.5 text-xs text-surface-800/60 text-right">
                          {op.epoch}
                        </td>
                        <td className="px-3 py-2.5">
                          <StatusDot status={op.status} />
                        </td>
                        <td className="px-5 py-2.5 text-xs text-surface-800/60 text-right">
                          {op.rewardTokens.toFixed(1)} ERG
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="px-5 py-3 border-t border-surface-100 text-xs text-surface-800/30">
                Mock data — real oracle pool data coming soon.
              </div>
            </div>

            {/* Epoch history chart */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
              <h2 className="text-base font-semibold text-surface-900 mb-1">
                Epoch Price History
              </h2>
              <p className="text-xs text-surface-800/40 mb-4">
                ERG/USD rate over the last 10 epochs
              </p>
              <EpochChart data={epochHistory} />
            </div>

            {/* Protocol info */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <h2 className="text-base font-semibold text-surface-900 mb-3">
                Protocol Info
              </h2>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div>
                  <h3 className="text-sm font-medium text-surface-900 mb-2">
                    Oracle Pool Pattern
                  </h3>
                  <p className="text-xs text-surface-800/60 leading-relaxed">
                    The oracle pool follows the EIP-23 specification. Multiple oracle operators post price
                    datapoints into individual oracle boxes. A refresh transaction collects these datapoints,
                    computes a median/average, and updates the pool box with the consensus rate. The pool box
                    holds the current rate in R4 and the epoch counter in R5, and is consumed as a data input
                    by DeFi transactions on Ergo.
                  </p>
                  <a
                    href="https://github.com/ergoplatform/eips/blob/master/eip-23.md"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 text-xs text-brand-600 hover:text-brand-700 mt-2 transition-colors"
                  >
                    View EIP-23 specification
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </a>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-surface-900 mb-3">
                    Parameters
                  </h3>
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-surface-800/50">Epoch Length</span>
                      <span className="text-xs font-medium text-surface-900 bg-surface-50 dark:bg-surface-900/50 px-2.5 py-1 rounded-md">
                        {ORACLE_PARAMS.epochLength}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-surface-800/50">Min Data Points</span>
                      <span className="text-xs font-medium text-surface-900 bg-surface-50 dark:bg-surface-900/50 px-2.5 py-1 rounded-md">
                        {ORACLE_PARAMS.minDataPoints}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-surface-800/50">Max Deviation</span>
                      <span className="text-xs font-medium text-surface-900 bg-surface-50 dark:bg-surface-900/50 px-2.5 py-1 rounded-md">
                        {ORACLE_PARAMS.maxDeviation}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-surface-800/50">Data Points / Epoch</span>
                      <span className="text-xs font-medium text-surface-900 bg-surface-50 dark:bg-surface-900/50 px-2.5 py-1 rounded-md">
                        {ORACLE_PARAMS.dataPointsPerEpoch}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </>
        )}
      </SuspenseWrap>
    </div>
  );
}
