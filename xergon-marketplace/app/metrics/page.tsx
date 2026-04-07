"use client";

import { useState, useEffect, useCallback, useMemo } from "react";

// ============================================================================
// Types
// ============================================================================

interface TPSDataPoint {
  timestamp: number;
  value: number;
}

interface LatencyBucket {
  range: string;
  count: number;
  label: string;
}

interface ModelMetrics {
  id: string;
  name: string;
  tps: number;
  avgLatency: number;
  p99Latency: number;
  errorRate: number;
  requests: number;
  status: "healthy" | "degraded" | "down";
}

interface SummaryMetrics {
  totalRequests: number;
  avgLatency: number;
  errorRate: number;
  activeModels: number;
  totalTPS: number;
  activeConnections: number;
}

type TimeRange = "1m" | "5m" | "15m" | "1h";
type RefreshInterval = 1000 | 2000 | 5000 | 10000;

// ============================================================================
// Constants
// ============================================================================

const MODEL_NAMES = [
  "Llama-3.1-70B",
  "Mixtral-8x7B",
  "Qwen-2.5-72B",
  "DeepSeek-V3",
  "Phi-4-MoE",
  "Gemma-2-27B",
  "Mistral-Large",
  "Command-R-Plus",
  "Codestral-22B",
  "Yi-Large",
];

const HEALTH_THRESHOLDS = {
  latency: { healthy: 200, degraded: 500 },
  errorRate: { healthy: 1, degraded: 5 },
  tps: { healthy: 10, degraded: 3 },
};

// ============================================================================
// Mock Data Generators
// ============================================================================

function randomBetween(min: number, max: number): number {
  return Math.random() * (max - min) + min;
}

function generateTPSHistory(points: number): TPSDataPoint[] {
  const now = Date.now();
  return Array.from({ length: points }, (_, i) => ({
    timestamp: now - (points - i) * 2000,
    value: randomBetween(80, 320),
  }));
}

function generateLatencyBuckets(): LatencyBucket[] {
  return [
    { range: "0-50", count: Math.floor(randomBetween(800, 2000)), label: "0-50ms" },
    { range: "50-100", count: Math.floor(randomBetween(2000, 5000)), label: "50-100ms" },
    { range: "100-200", count: Math.floor(randomBetween(3000, 6000)), label: "100-200ms" },
    { range: "200-500", count: Math.floor(randomBetween(1000, 3000)), label: "200-500ms" },
    { range: "500-1000", count: Math.floor(randomBetween(200, 800)), label: "500-1000ms" },
    { range: "1000+", count: Math.floor(randomBetween(20, 150)), label: "1000ms+" },
  ];
}

function generateModelMetrics(): ModelMetrics[] {
  return MODEL_NAMES.map((name) => {
    const avgLatency = randomBetween(80, 600);
    const errorRate = randomBetween(0, 6);
    const tps = randomBetween(5, 150);
    let status: ModelMetrics["status"] = "healthy";
    if (avgLatency > HEALTH_THRESHOLDS.latency.degraded || errorRate > HEALTH_THRESHOLDS.errorRate.degraded) {
      status = "down";
    } else if (avgLatency > HEALTH_THRESHOLDS.latency.healthy || errorRate > HEALTH_THRESHOLDS.errorRate.healthy) {
      status = "degraded";
    }
    return {
      id: name.toLowerCase().replace(/[^a-z0-9]/g, "-"),
      name,
      tps: Math.round(tps * 10) / 10,
      avgLatency: Math.round(avgLatency),
      p99Latency: Math.round(avgLatency * randomBetween(1.8, 3.5)),
      errorRate: Math.round(errorRate * 100) / 100,
      requests: Math.floor(randomBetween(1000, 50000)),
      status,
    };
  });
}

function generateSummary(): SummaryMetrics {
  return {
    totalRequests: Math.floor(randomBetween(100000, 500000)),
    avgLatency: Math.round(randomBetween(100, 400)),
    errorRate: Math.round(randomBetween(0.1, 3.5) * 100) / 100,
    activeModels: Math.floor(randomBetween(6, 10)),
    totalTPS: Math.round(randomBetween(150, 500)),
    activeConnections: Math.floor(randomBetween(500, 3000)),
  };
}

// ============================================================================
// Helpers
// ============================================================================

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function getStatusColor(status: ModelMetrics["status"]): string {
  switch (status) {
    case "healthy":
      return "bg-emerald-500";
    case "degraded":
      return "bg-amber-500";
    case "down":
      return "bg-red-500";
  }
}

function getStatusTextColor(status: ModelMetrics["status"]): string {
  switch (status) {
    case "healthy":
      return "text-emerald-600 dark:text-emerald-400";
    case "degraded":
      return "text-amber-600 dark:text-amber-400";
    case "down":
      return "text-red-600 dark:text-red-400";
  }
}

function getErrorRateColor(rate: number): string {
  if (rate <= HEALTH_THRESHOLDS.errorRate.healthy) return "text-emerald-600 dark:text-emerald-400";
  if (rate <= HEALTH_THRESHOLDS.errorRate.degraded) return "text-amber-600 dark:text-amber-400";
  return "text-red-600 dark:text-red-400";
}

function getLatencyColor(latency: number): string {
  if (latency <= HEALTH_THRESHOLDS.latency.healthy) return "text-emerald-600 dark:text-emerald-400";
  if (latency <= HEALTH_THRESHOLDS.latency.degraded) return "text-amber-600 dark:text-amber-400";
  return "text-red-600 dark:text-red-400";
}

function timeAgo(timestamp: number): string {
  const seconds = Math.floor((Date.now() - timestamp) / 1000);
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  return `${Math.floor(seconds / 60)}m ago`;
}

// ============================================================================
// MetricsHeader Component
// ============================================================================

function MetricsHeader({
  timeRange,
  onTimeRangeChange,
  refreshInterval,
  onRefreshIntervalChange,
  isLive,
  lastRefresh,
  onManualRefresh,
}: {
  timeRange: TimeRange;
  onTimeRangeChange: (r: TimeRange) => void;
  refreshInterval: RefreshInterval;
  onRefreshIntervalChange: (i: RefreshInterval) => void;
  isLive: boolean;
  lastRefresh: number;
  onManualRefresh: () => void;
}) {
  const timeRanges: TimeRange[] = ["1m", "5m", "15m", "1h"];
  const intervals: { value: RefreshInterval; label: string }[] = [
    { value: 1000, label: "1s" },
    { value: 2000, label: "2s" },
    { value: 5000, label: "5s" },
    { value: 10000, label: "10s" },
  ];

  return (
    <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-50">
          Inference Metrics
        </h1>
        <p className="mt-1 text-sm text-surface-600 dark:text-surface-400">
          Real-time performance monitoring across the inference fleet
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-1 rounded-lg bg-surface-100 p-1 dark:bg-surface-800">
          {timeRanges.map((r) => (
            <button
              key={r}
              onClick={() => onTimeRangeChange(r)}
              className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                timeRange === r
                  ? "bg-white text-surface-900 shadow-sm dark:bg-surface-700 dark:text-surface-50"
                  : "text-surface-600 hover:text-surface-900 dark:text-surface-400 dark:hover:text-surface-50"
              }`}
            >
              {r}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-2">
          <label className="text-xs text-surface-500">Refresh:</label>
          <select
            value={refreshInterval}
            onChange={(e) => onRefreshIntervalChange(Number(e.target.value) as RefreshInterval)}
            className="rounded-md border border-surface-200 bg-white px-2 py-1 text-xs text-surface-700 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {intervals.map((i) => (
              <option key={i.value} value={i.value}>
                {i.label}
              </option>
            ))}
          </select>
        </div>
        <button
          onClick={onManualRefresh}
          className="rounded-lg border border-surface-200 bg-white px-3 py-1.5 text-xs font-medium text-surface-700 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300 dark:hover:bg-surface-700"
        >
          Refresh
        </button>
        {isLive && (
          <div className="flex items-center gap-1.5">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-500 animate-pulse" />
            <span className="text-xs text-surface-500">Live</span>
          </div>
        )}
        <span className="text-xs text-surface-400">{timeAgo(lastRefresh)}</span>
      </div>
    </div>
  );
}

// ============================================================================
// MetricCard Component
// ============================================================================

function MetricCard({
  title,
  value,
  subtitle,
  trend,
  icon,
  color = "text-surface-900 dark:text-surface-50",
}: {
  title: string;
  value: string;
  subtitle?: string;
  trend?: "up" | "down" | "stable";
  icon: string;
  color?: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm transition-shadow hover:shadow-md dark:border-surface-800 dark:bg-surface-900">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <p className="text-xs font-medium uppercase tracking-wider text-surface-500">
            {title}
          </p>
          <p className={`mt-2 text-2xl font-bold ${color}`}>{value}</p>
          {subtitle && (
            <p className="mt-1 text-xs text-surface-500">{subtitle}</p>
          )}
        </div>
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-surface-100 text-lg dark:bg-surface-800">
          {icon}
        </div>
      </div>
      {trend && (
        <div className="mt-3 flex items-center gap-1">
          {trend === "up" && (
            <span className="text-xs font-medium text-emerald-600 dark:text-emerald-400">
              &#9650; Trending up
            </span>
          )}
          {trend === "down" && (
            <span className="text-xs font-medium text-red-500">
              &#9660; Trending down
            </span>
          )}
          {trend === "stable" && (
            <span className="text-xs font-medium text-surface-500">
              &#8212; Stable
            </span>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// TPSChart Component - SVG Line Chart
// ============================================================================

function TPSChart({
  data,
  width = 800,
  height = 300,
}: {
  data: TPSDataPoint[];
  width?: number;
  height?: number;
}) {
  const padding = { top: 20, right: 20, bottom: 40, left: 60 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const values = data.map((d) => d.value);
  const minVal = Math.max(0, Math.min(...values) - 20);
  const maxVal = Math.max(...values) + 20;
  const range = maxVal - minVal || 1;

  const points = data.map((d, i) => ({
    x: padding.left + (i / Math.max(data.length - 1, 1)) * chartW,
    y: padding.top + chartH - ((d.value - minVal) / range) * chartH,
  }));

  const linePath =
    points.length > 0
      ? points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ")
      : "";

  const areaPath =
    points.length > 0
      ? `${linePath} L ${points[points.length - 1].x} ${padding.top + chartH} L ${points[0].x} ${padding.top + chartH} Z`
      : "";

  // Grid lines
  const gridLines = 5;
  const gridLabels: string[] = [];
  for (let i = 0; i <= gridLines; i++) {
    const val = minVal + (range * i) / gridLines;
    gridLabels.push(Math.round(val).toString());
  }

  // Time labels
  const timeLabels = data.filter((_, i) => i % Math.max(Math.floor(data.length / 6), 1) === 0);
  const timeLabelPoints = timeLabels.map((_, i) => {
    const idx = i * Math.max(Math.floor(data.length / 6), 1);
    return padding.left + (idx / Math.max(data.length - 1, 1)) * chartW;
  });

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Tokens Per Second (TPS)
        </h3>
        <span className="rounded-full bg-emerald-100 px-2.5 py-0.5 text-xs font-medium text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400">
          Real-time
        </span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <linearGradient id="tpsGradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#10b981" stopOpacity="0.3" />
            <stop offset="100%" stopColor="#10b981" stopOpacity="0.02" />
          </linearGradient>
        </defs>

        {/* Grid */}
        {Array.from({ length: gridLines + 1 }, (_, i) => {
          const y = padding.top + (chartH * i) / gridLines;
          return (
            <g key={`grid-${i}`}>
              <line
                x1={padding.left}
                y1={y}
                x2={width - padding.right}
                y2={y}
                stroke="currentColor"
                className="text-surface-200 dark:text-surface-700"
                strokeWidth={1}
                strokeDasharray="4 4"
              />
              <text
                x={padding.left - 8}
                y={y + 4}
                textAnchor="end"
                className="fill-surface-500 dark:fill-surface-400"
                fontSize={11}
              >
                {gridLabels[i]}
              </text>
            </g>
          );
        })}

        {/* Area fill */}
        {areaPath && <path d={areaPath} fill="url(#tpsGradient)" />}

        {/* Line */}
        {linePath && (
          <path
            d={linePath}
            fill="none"
            stroke="#10b981"
            strokeWidth={2.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        )}

        {/* Current value dot */}
        {points.length > 0 && (
          <>
            <circle
              cx={points[points.length - 1].x}
              cy={points[points.length - 1].y}
              r={4}
              fill="#10b981"
              stroke="white"
              strokeWidth={2}
            />
            <circle
              cx={points[points.length - 1].x}
              cy={points[points.length - 1].y}
              r={8}
              fill="#10b981"
              opacity={0.2}
            />
          </>
        )}

        {/* X axis labels */}
        {timeLabelPoints.map((x, i) => (
          <text
            key={`time-${i}`}
            x={x}
            y={height - 8}
            textAnchor="middle"
            className="fill-surface-400 dark:fill-surface-500"
            fontSize={10}
          >
            -{((data.length - 1 - i * Math.max(Math.floor(data.length / 6), 1)) * 2)}s
          </text>
        ))}
      </svg>
    </div>
  );
}

// ============================================================================
// LatencyHistogram Component - SVG Bar Chart
// ============================================================================

function LatencyHistogram({ data }: { data: LatencyBucket[] }) {
  const width = 800;
  const height = 280;
  const padding = { top: 20, right: 20, bottom: 50, left: 60 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const maxCount = Math.max(...data.map((d) => d.count), 1);
  const barWidth = chartW / data.length - 12;

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Latency Distribution
        </h3>
        <span className="text-xs text-surface-500">
          {formatNumber(data.reduce((sum, b) => sum + b.count, 0))} total samples
        </span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Y axis grid */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac, i) => {
          const y = padding.top + chartH * (1 - frac);
          const val = Math.round(maxCount * frac);
          return (
            <g key={`hgrid-${i}`}>
              <line
                x1={padding.left}
                y1={y}
                x2={width - padding.right}
                y2={y}
                stroke="currentColor"
                className="text-surface-200 dark:text-surface-700"
                strokeWidth={1}
                strokeDasharray="4 4"
              />
              <text
                x={padding.left - 8}
                y={y + 4}
                textAnchor="end"
                className="fill-surface-500 dark:fill-surface-400"
                fontSize={11}
              >
                {formatNumber(val)}
              </text>
            </g>
          );
        })}

        {/* Bars */}
        {data.map((bucket, i) => {
          const x = padding.left + (i * chartW) / data.length + 6;
          const barH = (bucket.count / maxCount) * chartH;
          const y = padding.top + chartH - barH;

          // Color based on latency range
          let fillColor = "#10b981"; // green
          if (i >= 4) fillColor = "#ef4444"; // red
          else if (i >= 3) fillColor = "#f59e0b"; // yellow
          else if (i >= 2) fillColor = "#3b82f6"; // blue

          return (
            <g key={`bar-${i}`}>
              <rect
                x={x}
                y={y}
                width={barWidth}
                height={barH}
                fill={fillColor}
                rx={4}
                ry={4}
                opacity={0.85}
              />
              <text
                x={x + barWidth / 2}
                y={y - 6}
                textAnchor="middle"
                className="fill-surface-700 dark:fill-surface-300"
                fontSize={10}
                fontWeight="500"
              >
                {formatNumber(bucket.count)}
              </text>
              <text
                x={x + barWidth / 2}
                y={height - 8}
                textAnchor="middle"
                className="fill-surface-500 dark:fill-surface-400"
                fontSize={10}
              >
                {bucket.label}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ============================================================================
// ConnectionsGauge Component - Circular SVG Gauge
// ============================================================================

function ConnectionsGauge({
  value,
  max = 5000,
  label = "Active Connections",
}: {
  value: number;
  max?: number;
  label?: string;
}) {
  const size = 200;
  const strokeWidth = 16;
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const percentage = Math.min(value / max, 1);
  const offset = circumference * (1 - percentage);

  // Color based on percentage
  let strokeColor = "#10b981";
  if (percentage > 0.8) strokeColor = "#ef4444";
  else if (percentage > 0.6) strokeColor = "#f59e0b";

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-4 text-sm font-semibold text-surface-900 dark:text-surface-50">
        {label}
      </h3>
      <div className="flex flex-col items-center">
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
          {/* Background circle */}
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke="currentColor"
            className="text-surface-200 dark:text-surface-700"
            strokeWidth={strokeWidth}
          />
          {/* Progress circle */}
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={strokeColor}
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={offset}
            transform={`rotate(-90 ${size / 2} ${size / 2})`}
            className="transition-all duration-700 ease-out"
          />
          {/* Center text */}
          <text
            x={size / 2}
            y={size / 2 - 8}
            textAnchor="middle"
            className="fill-surface-900 dark:fill-surface-50"
            fontSize={32}
            fontWeight="bold"
          >
            {formatNumber(value)}
          </text>
          <text
            x={size / 2}
            y={size / 2 + 16}
            textAnchor="middle"
            className="fill-surface-500 dark:fill-surface-400"
            fontSize={13}
          >
            of {formatNumber(max)}
          </text>
        </svg>
        <div className="mt-2 flex items-center gap-2">
          <span
            className="inline-block h-2.5 w-2.5 rounded-full"
            style={{ backgroundColor: strokeColor }}
          />
          <span className="text-xs text-surface-500">
            {(percentage * 100).toFixed(1)}% capacity
          </span>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// RequestRateChart Component - SVG Line Chart
// ============================================================================

function RequestRateChart({
  data,
  width = 800,
  height = 200,
}: {
  data: TPSDataPoint[];
  width?: number;
  height?: number;
}) {
  const padding = { top: 20, right: 20, bottom: 30, left: 60 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const values = data.map((d) => d.value);
  const minVal = Math.max(0, Math.min(...values) - 10);
  const maxVal = Math.max(...values) + 10;
  const range = maxVal - minVal || 1;

  const points = data.map((d, i) => ({
    x: padding.left + (i / Math.max(data.length - 1, 1)) * chartW,
    y: padding.top + chartH - ((d.value - minVal) / range) * chartH,
  }));

  const linePath =
    points.length > 0
      ? points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ")
      : "";

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-4 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Request Rate (req/s)
      </h3>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <linearGradient id="reqGradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#3b82f6" stopOpacity="0.25" />
            <stop offset="100%" stopColor="#3b82f6" stopOpacity="0.02" />
          </linearGradient>
        </defs>

        {/* Grid */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac, i) => {
          const y = padding.top + chartH * (1 - frac);
          return (
            <line
              key={`rgrid-${i}`}
              x1={padding.left}
              y1={y}
              x2={width - padding.right}
              y2={y}
              stroke="currentColor"
              className="text-surface-200 dark:text-surface-700"
              strokeWidth={1}
              strokeDasharray="4 4"
            />
          );
        })}

        {linePath && <path d={`${linePath} L ${points[points.length - 1].x} ${padding.top + chartH} L ${points[0].x} ${padding.top + chartH} Z`} fill="url(#reqGradient)" />}
        {linePath && (
          <path
            d={linePath}
            fill="none"
            stroke="#3b82f6"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        )}
        {points.length > 0 && (
          <circle
            cx={points[points.length - 1].x}
            cy={points[points.length - 1].y}
            r={3.5}
            fill="#3b82f6"
            stroke="white"
            strokeWidth={2}
          />
        )}
      </svg>
    </div>
  );
}

// ============================================================================
// ErrorRateTracker Component
// ============================================================================

function ErrorRateTracker({ rate, history }: { rate: number; history: number[] }) {
  const width = 800;
  const height = 140;
  const padding = { top: 15, right: 20, bottom: 25, left: 60 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const maxVal = Math.max(...history, 1);
  const barW = chartW / history.length - 2;

  const errorColor =
    rate <= HEALTH_THRESHOLDS.errorRate.healthy
      ? "#10b981"
      : rate <= HEALTH_THRESHOLDS.errorRate.degraded
      ? "#f59e0b"
      : "#ef4444";

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Error Rate
        </h3>
        <span
          className="text-lg font-bold"
          style={{ color: errorColor }}
        >
          {rate.toFixed(2)}%
        </span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Threshold line */}
        <line
          x1={padding.left}
          y1={padding.top + chartH * (1 - HEALTH_THRESHOLDS.errorRate.healthy / maxVal)}
          x2={width - padding.right}
          y2={padding.top + chartH * (1 - HEALTH_THRESHOLDS.errorRate.healthy / maxVal)}
          stroke="#10b981"
          strokeWidth={1}
          strokeDasharray="6 3"
          opacity={0.5}
        />
        <line
          x1={padding.left}
          y1={padding.top + chartH * (1 - HEALTH_THRESHOLDS.errorRate.degraded / maxVal)}
          x2={width - padding.right}
          y2={padding.top + chartH * (1 - HEALTH_THRESHOLDS.errorRate.degraded / maxVal)}
          stroke="#f59e0b"
          strokeWidth={1}
          strokeDasharray="6 3"
          opacity={0.5}
        />

        {history.map((val, i) => {
          const x = padding.left + (i * chartW) / history.length + 1;
          const barH = (val / maxVal) * chartH;
          const y = padding.top + chartH - barH;
          const c =
            val <= HEALTH_THRESHOLDS.errorRate.healthy
              ? "#10b981"
              : val <= HEALTH_THRESHOLDS.errorRate.degraded
              ? "#f59e0b"
              : "#ef4444";
          return (
            <rect key={i} x={x} y={y} width={barW} height={barH} fill={c} rx={2} opacity={0.8} />
          );
        })}
      </svg>
      <div className="mt-2 flex items-center gap-4 text-xs text-surface-500">
        <div className="flex items-center gap-1">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-500" />
          Healthy (&le;{HEALTH_THRESHOLDS.errorRate.healthy}%)
        </div>
        <div className="flex items-center gap-1">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-500" />
          Degraded (&le;{HEALTH_THRESHOLDS.errorRate.degraded}%)
        </div>
        <div className="flex items-center gap-1">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-red-500" />
          Critical (&gt;{HEALTH_THRESHOLDS.errorRate.degraded}%)
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// ModelBreakdown Component - Table
// ============================================================================

function ModelBreakdown({ models }: { models: ModelMetrics[] }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="border-b border-surface-200 px-5 py-4 dark:border-surface-800">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Model Breakdown
        </h3>
        <p className="mt-0.5 text-xs text-surface-500">
          Per-model inference metrics across all providers
        </p>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="border-b border-surface-200 bg-surface-50 dark:border-surface-800 dark:bg-surface-800/50">
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500">
                Model
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500">
                Status
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500 text-right">
                TPS
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500 text-right">
                Avg Latency
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500 text-right">
                P99 Latency
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500 text-right">
                Error Rate
              </th>
              <th className="px-5 py-3 text-xs font-medium uppercase tracking-wider text-surface-500 text-right">
                Requests
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-surface-100 dark:divide-surface-800">
            {models.map((model) => (
              <tr
                key={model.id}
                className="transition-colors hover:bg-surface-50 dark:hover:bg-surface-800/30"
              >
                <td className="px-5 py-3 font-medium text-surface-900 dark:text-surface-50">
                  {model.name}
                </td>
                <td className="px-5 py-3">
                  <div className="flex items-center gap-2">
                    <span className={`inline-block h-2 w-2 rounded-full ${getStatusColor(model.status)}`} />
                    <span className={`text-xs capitalize ${getStatusTextColor(model.status)}`}>
                      {model.status}
                    </span>
                  </div>
                </td>
                <td className="px-5 py-3 text-right font-mono text-surface-700 dark:text-surface-300">
                  {model.tps.toFixed(1)}
                </td>
                <td className={`px-5 py-3 text-right font-mono ${getLatencyColor(model.avgLatency)}`}>
                  {model.avgLatency}ms
                </td>
                <td className="px-5 py-3 text-right font-mono text-surface-700 dark:text-surface-300">
                  {model.p99Latency}ms
                </td>
                <td className={`px-5 py-3 text-right font-mono ${getErrorRateColor(model.errorRate)}`}>
                  {model.errorRate.toFixed(2)}%
                </td>
                <td className="px-5 py-3 text-right font-mono text-surface-700 dark:text-surface-300">
                  {formatNumber(model.requests)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ============================================================================
// Main Page Component
// ============================================================================

export default function MetricsPage() {
  const [timeRange, setTimeRange] = useState<TimeRange>("5m");
  const [refreshInterval, setRefreshInterval] = useState<RefreshInterval>(2000);
  const [lastRefresh, setLastRefresh] = useState(Date.now());
  const [isLive, setIsLive] = useState(true);

  // Metrics state
  const [tpsHistory, setTpsHistory] = useState<TPSDataPoint[]>(() => generateTPSHistory(30));
  const [latencyBuckets, setLatencyBuckets] = useState<LatencyBucket[]>(() => generateLatencyBuckets());
  const [modelMetrics, setModelMetrics] = useState<ModelMetrics[]>(() => generateModelMetrics());
  const [summary, setSummary] = useState<SummaryMetrics>(() => generateSummary());
  const [requestRateHistory, setRequestRateHistory] = useState<TPSDataPoint[]>(() =>
    Array.from({ length: 20 }, (_, i) => ({
      timestamp: Date.now() - (20 - i) * 2000,
      value: randomBetween(200, 800),
    }))
  );
  const [errorHistory, setErrorHistory] = useState<number[]>(() =>
    Array.from({ length: 15 }, () => randomBetween(0.1, 4))
  );

  const refreshData = useCallback(() => {
    setTpsHistory((prev) => {
      const next = [...prev.slice(1), { timestamp: Date.now(), value: randomBetween(80, 320) }];
      return next;
    });
    setLatencyBuckets(generateLatencyBuckets());
    setModelMetrics(generateModelMetrics());
    setSummary(generateSummary());
    setRequestRateHistory((prev) => {
      const next = [...prev.slice(1), { timestamp: Date.now(), value: randomBetween(200, 800) }];
      return next;
    });
    setErrorHistory((prev) => {
      const next = [...prev.slice(1), randomBetween(0.1, 4)];
      return next;
    });
    setLastRefresh(Date.now());
  }, []);

  useEffect(() => {
    if (!isLive) return;
    const interval = setInterval(refreshData, refreshInterval);
    return () => clearInterval(interval);
  }, [isLive, refreshInterval, refreshData]);

  // Compute data points based on time range
  const pointsForRange = useMemo(() => {
    switch (timeRange) {
      case "1m": return 30;
      case "5m": return 30;
      case "15m": return 45;
      case "1h": return 60;
    }
  }, [timeRange]);

  // Summary trend simulation
  const trendSimulation = useMemo(() => {
    const trends = ["up" as const, "down" as const, "stable" as const];
    return {
      requests: trends[Math.floor(Math.random() * 3)],
      latency: trends[Math.floor(Math.random() * 3)],
      errors: trends[Math.floor(Math.random() * 3)],
      models: "stable" as const,
    };
  }, [lastRefresh]);

  return (
    <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      {/* Header */}
      <MetricsHeader
        timeRange={timeRange}
        onTimeRangeChange={setTimeRange}
        refreshInterval={refreshInterval}
        onRefreshIntervalChange={setRefreshInterval}
        isLive={isLive}
        lastRefresh={lastRefresh}
        onManualRefresh={refreshData}
      />

      {/* Summary Cards */}
      <div className="mt-8 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          title="Total Requests"
          value={formatNumber(summary.totalRequests)}
          subtitle="Last 24 hours"
          trend={trendSimulation.requests}
          icon="&#x1F4CA;"
          color={trendSimulation.requests === "up" ? "text-emerald-600 dark:text-emerald-400" : undefined}
        />
        <MetricCard
          title="Avg Latency"
          value={`${summary.avgLatency}ms`}
          subtitle="P50 across fleet"
          trend={trendSimulation.latency}
          icon="&#x23F1;"
          color={getLatencyColor(summary.avgLatency)}
        />
        <MetricCard
          title="Error Rate"
          value={`${summary.errorRate.toFixed(2)}%`}
          subtitle="Last 5 minutes"
          trend={trendSimulation.errors}
          icon="&#x26A0;"
          color={getErrorRateColor(summary.errorRate)}
        />
        <MetricCard
          title="Active Models"
          value={summary.activeModels.toString()}
          subtitle={`of ${MODEL_NAMES.length} deployed`}
          trend={trendSimulation.models}
          icon="&#x1F9E0;"
        />
      </div>

      {/* Charts Row 1 */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <TPSChart data={tpsHistory} />
        </div>
        <div>
          <ConnectionsGauge value={summary.activeConnections} max={5000} />
        </div>
      </div>

      {/* Charts Row 2 */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        <LatencyHistogram data={latencyBuckets} />
        <RequestRateChart data={requestRateHistory} />
      </div>

      {/* Error Rate */}
      <div className="mt-6">
        <ErrorRateTracker rate={summary.errorRate} history={errorHistory} />
      </div>

      {/* Model Breakdown Table */}
      <div className="mt-6">
        <ModelBreakdown models={modelMetrics} />
      </div>

      {/* Footer info */}
      <div className="mt-8 flex items-center justify-between rounded-lg bg-surface-50 px-4 py-3 dark:bg-surface-800/50">
        <p className="text-xs text-surface-500">
          Data refreshes every {refreshInterval / 1000}s. All metrics are simulated for demonstration.
        </p>
        <div className="flex items-center gap-3 text-xs text-surface-400">
          <span>Fleet TPS: <strong className="text-surface-700 dark:text-surface-300">{summary.totalTPS}</strong></span>
          <span>|</span>
          <span>Models: <strong className="text-surface-700 dark:text-surface-300">{summary.activeModels}/{MODEL_NAMES.length}</strong></span>
        </div>
      </div>
    </div>
  );
}
