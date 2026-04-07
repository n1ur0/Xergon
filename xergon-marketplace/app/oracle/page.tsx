"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OracleRateResponse {
  rate: number;
  epoch?: number;
  poolBoxId?: string;
  timestamp?: string;
}

interface PriceHistoryPoint {
  price: number;
  timestamp: number;
}

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
// Constants
// ---------------------------------------------------------------------------

const ORACLE_PARAMS = {
  epochLength: "12 blocks (~40 min)",
  minDataPoints: 4,
  maxDeviation: "5%",
  dataPointsPerEpoch: 6,
};

const PRICE_HISTORY_KEY = "xergon_oracle_price_history";
const MAX_HISTORY_POINTS = 24;
const REFRESH_INTERVAL_MS = 60_000;

// ---------------------------------------------------------------------------
// localStorage helpers
// ---------------------------------------------------------------------------

function loadPriceHistory(): PriceHistoryPoint[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(PRICE_HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (p: unknown) =>
        p &&
        typeof p === "object" &&
        typeof (p as PriceHistoryPoint).price === "number" &&
        typeof (p as PriceHistoryPoint).timestamp === "number"
    );
  } catch {
    return [];
  }
}

function savePriceHistory(history: PriceHistoryPoint[]) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(PRICE_HISTORY_KEY, JSON.stringify(history.slice(-MAX_HISTORY_POINTS)));
  } catch {
    // quota exceeded — ignore
  }
}

// ---------------------------------------------------------------------------
// Mock operator data (real operator data would come from on-chain queries)
// ---------------------------------------------------------------------------

const MOCK_OPERATORS: OracleOperator[] = [
  { name: "ergo-oracle-1", lastDatapoint: "--", epoch: 0, status: "active", rewardTokens: 12.4 },
  { name: "ergo-oracle-2", lastDatapoint: "--", epoch: 0, status: "active", rewardTokens: 11.8 },
  { name: "ergo-oracle-3", lastDatapoint: "--", epoch: 0, status: "active", rewardTokens: 13.1 },
  { name: "ergo-oracle-4", lastDatapoint: "--", epoch: 0, status: "active", rewardTokens: 10.2 },
  { name: "ergo-oracle-5", lastDatapoint: "--", epoch: 0, status: "stale", rewardTokens: 8.7 },
  { name: "ergo-oracle-6", lastDatapoint: "--", epoch: 0, status: "offline", rewardTokens: 3.1 },
];

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
      {/* Sparkline skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <SkeletonPulse className="h-5 w-36 mb-4" />
        <SkeletonPulse className="h-[120px] w-full" />
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
      {/* Protocol info skeleton */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
        <SkeletonPulse className="h-5 w-32 mb-4" />
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="space-y-2">
            <SkeletonPulse className="h-4 w-full" />
            <SkeletonPulse className="h-4 w-3/4" />
          </div>
          <div className="space-y-3">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="flex items-center justify-between">
                <SkeletonPulse className="h-4 w-32" />
                <SkeletonPulse className="h-6 w-24 rounded-md" />
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Error state
// ---------------------------------------------------------------------------

function OracleError({ onRetry }: { onRetry: () => void }) {
  return (
    <div className="space-y-6">
      <div className="rounded-xl border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 p-8 text-center">
        <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-red-100 dark:bg-red-900/30">
          <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-red-500" aria-hidden="true">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
        </div>
        <h2 className="text-lg font-semibold text-surface-900 mb-1">Oracle Unavailable</h2>
        <p className="text-sm text-surface-800/60 mb-4">
          Unable to reach the oracle service. This may be a temporary issue.
        </p>
        <button
          onClick={onRetry}
          className="inline-flex items-center gap-2 rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <polyline points="23 4 23 10 17 10" />
            <polyline points="1 20 1 14 7 14" />
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
          </svg>
          Retry
        </button>
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
// Sparkline chart (SVG, last 24 data points)
// ---------------------------------------------------------------------------

function SparklineChart({ data }: { data: PriceHistoryPoint[] }) {
  if (data.length < 2) {
    return (
      <div className="flex items-center justify-center h-[120px] text-xs text-surface-800/40">
        Collecting price data...
      </div>
    );
  }

  const width = 600;
  const height = 120;
  const padX = 4;
  const padY = 8;
  const plotW = width - padX * 2;
  const plotH = height - padY * 2;

  const prices = data.map((d) => d.price);
  const minP = Math.min(...prices) - 0.01;
  const maxP = Math.max(...prices) + 0.01;
  const range = maxP - minP || 1;

  const points = data.map((d, i) => {
    const x = padX + (data.length > 1 ? (i / (data.length - 1)) * plotW : plotW / 2);
    const y = padY + plotH - ((d.price - minP) / range) * plotH;
    return { x, y, ...d };
  });

  const areaPath =
    `M${points[0].x.toFixed(1)},${(padY + plotH).toFixed(1)} ` +
    points.map((p) => `L${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ") +
    ` L${points[points.length - 1].x.toFixed(1)},${(padY + plotH).toFixed(1)} Z`;

  // Determine line color based on trend
  const firstPrice = prices[0];
  const lastPrice = prices[prices.length - 1];
  const isUp = lastPrice >= firstPrice;
  const lineColor = isUp ? "stroke-emerald-500" : "stroke-red-500";
  const fillColor = isUp ? "fill-emerald-500/10" : "fill-red-500/10";

  // Format time for tooltip
  function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="w-full h-auto"
      role="img"
      aria-label={`ERG/USD sparkline chart with ${data.length} data points, last price $${lastPrice.toFixed(2)}`}
    >
      {/* Area fill */}
      <path d={areaPath} className={fillColor} />

      {/* Line */}
      <polyline
        points={points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ")}
        fill="none"
        className={lineColor}
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Latest point */}
      {points.length > 0 && (
        <circle
          cx={points[points.length - 1].x}
          cy={points[points.length - 1].y}
          r={4}
          className={isUp ? "fill-emerald-500 stroke-surface-0" : "fill-red-500 stroke-surface-0"}
          strokeWidth={2}
        />
      )}

      {/* Price labels at start and end */}
      <text
        x={padX}
        y={height - 2}
        textAnchor="start"
        className="fill-surface-800/30 text-[10px]"
      >
        ${firstPrice.toFixed(2)}
      </text>
      <text
        x={width - padX}
        y={height - 2}
        textAnchor="end"
        className="fill-surface-800/30 text-[10px]"
      >
        ${lastPrice.toFixed(2)}
      </text>

      {/* Time labels */}
      <text
        x={padX}
        y={10}
        textAnchor="start"
        className="fill-surface-800/30 text-[9px]"
      >
        {formatTime(points[0].timestamp)}
      </text>
      <text
        x={width - padX}
        y={10}
        textAnchor="end"
        className="fill-surface-800/30 text-[9px]"
      >
        {formatTime(points[points.length - 1].timestamp)}
      </text>
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Epoch chart (for mock operator data)
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

  const yTicks = [minP, (minP + maxP) / 2, maxP];

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      className="w-full h-auto"
      role="img"
      aria-label={`ERG/USD price chart over ${data.length} epochs`}
    >
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

      <path
        d={
          `M${points[0].x.toFixed(1)},${(padY + plotH).toFixed(1)} ` +
          points.map((p) => `L${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ") +
          ` L${points[points.length - 1].x.toFixed(1)},${(padY + plotH).toFixed(1)} Z`
        }
        className="fill-brand-500/10"
      />

      <polyline
        points={points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ")}
        fill="none"
        className="stroke-brand-500"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
      />

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
// Compute price change from history
// ---------------------------------------------------------------------------

function computePriceChange(history: PriceHistoryPoint[]): { percent: number; direction: "up" | "down" | "neutral" } {
  if (history.length < 2) return { percent: 0, direction: "neutral" };
  const latest = history[history.length - 1].price;
  const first = history[0].price;
  if (first === 0) return { percent: 0, direction: "neutral" };
  const percent = ((latest - first) / first) * 100;
  return {
    percent: Math.abs(percent),
    direction: percent > 0.1 ? "up" : percent < -0.1 ? "down" : "neutral",
  };
}

// ---------------------------------------------------------------------------
// Price change indicator
// ---------------------------------------------------------------------------

function PriceChangeIndicator({ direction, percent }: { direction: "up" | "down" | "neutral"; percent: number }) {
  if (direction === "neutral") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/60">
        <span className="text-surface-800/30">&mdash;</span>
        0.00%
      </span>
    );
  }

  const isUp = direction === "up";
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full border px-3 py-1 text-xs font-medium ${
        isUp
          ? "border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 text-emerald-700 dark:text-emerald-400"
          : "border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 text-red-700 dark:text-red-400"
      }`}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
        style={{ transform: isUp ? "rotate(0deg)" : "rotate(180deg)" }}
      >
        <polyline points="18 15 12 9 6 15" />
      </svg>
      {isUp ? "+" : "-"}{percent.toFixed(2)}%
    </span>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function OraclePage() {
  const [price, setPrice] = useState<number | null>(null);
  const [epoch, setEpoch] = useState<number>(0);
  const [poolBoxId, setPoolBoxId] = useState<string>("");
  const [lastRefresh, setLastRefresh] = useState<string>("");
  const [priceHistory, setPriceHistory] = useState<PriceHistoryPoint[]>([]);
  const [operators, setOperators] = useState<OracleOperator[]>([]);
  const [epochHistory, setEpochHistory] = useState<EpochDataPoint[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isError, setIsError] = useState(false);
  const [fetchCount, setFetchCount] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const loadData = useCallback(() => {
    setIsLoading(true);
    setIsError(false);

    Promise.all([
      fetch("/api/xergon-agent/api/oracle/rate", { cache: "no-store" })
        .then((res) => {
          if (!res.ok) throw new Error(`Oracle rate returned ${res.status}`);
          return res.json();
        })
        .then((data: OracleRateResponse) => {
          if (typeof data.rate !== "number" || data.rate <= 0) {
            throw new Error("Invalid oracle rate");
          }
          setPrice(data.rate);
          setEpoch(data.epoch ?? 0);
          setPoolBoxId(data.poolBoxId ?? "");

          const ts = data.timestamp
            ? new Date(data.timestamp)
            : new Date();
          setLastRefresh(ts.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }));

          // Update price history
          const newPoint: PriceHistoryPoint = {
            price: data.rate,
            timestamp: Date.now(),
          };
          const current = loadPriceHistory();
          const updated = [...current, newPoint].slice(-MAX_HISTORY_POINTS);
          savePriceHistory(updated);
          setPriceHistory(updated);

          // Build epoch history from stored points
          setEpochHistory(
            updated.map((p, i) => ({
              epoch: (data.epoch ?? 0) - (updated.length - 1 - i),
              price: p.price,
            }))
          );

          // Update operator data with current price
          setOperators(
            MOCK_OPERATORS.map((op) => ({
              ...op,
              epoch: data.epoch ?? 0,
              lastDatapoint: `$${data.rate.toFixed(2)}`,
            }))
          );
        }),
    ])
      .then(() => {
        setIsLoading(false);
      })
      .catch(() => {
        setIsError(true);
        setIsLoading(false);
        // Still load cached history for display
        const cached = loadPriceHistory();
        setPriceHistory(cached);
      });
  }, []);

  // Initial load + auto-refresh
  useEffect(() => {
    loadData();
    timerRef.current = setInterval(loadData, REFRESH_INTERVAL_MS);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [loadData]);

  // Force retry
  const handleRetry = useCallback(() => {
    setFetchCount((c) => c + 1);
    loadData();
  }, [loadData]);

  const { direction, percent } = computePriceChange(priceHistory);

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
        {isError && !isLoading && price === null ? (
          <OracleError onRetry={handleRetry} />
        ) : isLoading && price === null ? (
          <OracleSkeleton />
        ) : (
          <>
            {/* Current rate hero card */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
              <div className="flex flex-col sm:flex-row sm:items-end gap-4">
                <div>
                  <p className="text-xs text-surface-800/50 mb-1">ERG / USD</p>
                  <div className="flex items-baseline gap-3">
                    <p className="text-5xl font-bold text-surface-900 tracking-tight">
                      {price !== null ? `$${price.toFixed(2)}` : "--"}
                    </p>
                    <PriceChangeIndicator direction={direction} percent={percent} />
                  </div>
                </div>
                <div className="sm:ml-auto flex flex-wrap gap-3">
                  {epoch > 0 && (
                    <span className="inline-flex items-center gap-1.5 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/60">
                      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-surface-800/40" aria-hidden="true">
                        <circle cx="12" cy="12" r="10" />
                        <polyline points="12 6 12 12 16 14" />
                      </svg>
                      Epoch {epoch}
                    </span>
                  )}
                  {lastRefresh && (
                    <span className="inline-flex items-center gap-1.5 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/60">
                      Last refresh: {lastRefresh}
                    </span>
                  )}
                  <span className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium ${
                    !isError
                      ? "border-emerald-200 bg-emerald-50 dark:border-emerald-800/40 dark:bg-emerald-950/20 text-emerald-700 dark:text-emerald-400"
                      : "border-amber-200 bg-amber-50 dark:border-amber-800/40 dark:bg-amber-950/20 text-amber-700 dark:text-amber-400"
                  }`}>
                    <span className={`h-1.5 w-1.5 rounded-full ${!isError ? "bg-emerald-500" : "bg-amber-500 animate-pulse"}`} aria-hidden="true" />
                    {!isError ? "Live" : "Reconnecting..."}
                  </span>
                </div>
              </div>

              {/* Pool box ID link */}
              {poolBoxId && (
                <div className="mt-4 pt-4 border-t border-surface-100">
                  <span className="text-xs text-surface-800/40">Oracle Pool Box: </span>
                  <a
                    href={`https://explorer.ergoplatform.com/en/boxes/${poolBoxId}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 text-xs font-mono text-brand-600 hover:text-brand-700 transition-colors"
                  >
                    {poolBoxId.slice(0, 16)}...{poolBoxId.slice(-8)}
                    <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </a>
                </div>
              )}
            </div>

            {/* Price history sparkline */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
              <div className="flex items-center justify-between mb-4">
                <div>
                  <h2 className="text-base font-semibold text-surface-900">
                    Price History
                  </h2>
                  <p className="text-xs text-surface-800/40 mt-0.5">
                    Last {priceHistory.length} data points (stored locally)
                  </p>
                </div>
                <span className="inline-flex items-center gap-1.5 rounded-full border border-surface-200 bg-surface-0 px-3 py-1 text-xs text-surface-800/40">
                  Auto-refreshes every 60s
                </span>
              </div>
              <SparklineChart data={priceHistory} />
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
              {epochHistory.length > 0 && (
                <div className="px-5 py-4 border-t border-surface-100">
                  <h3 className="text-sm font-medium text-surface-900 mb-2">
                    Epoch Price History
                  </h3>
                  <EpochChart data={epochHistory.slice(-10)} />
                </div>
              )}
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
