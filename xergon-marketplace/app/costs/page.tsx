"use client";

import { useState, useEffect, useCallback, useMemo } from "react";

// ============================================================================
// Types
// ============================================================================

interface CostBreakdownItem {
  category: string;
  amount: number;
  percentage: number;
  color: string;
}

interface CostHistoryPoint {
  date: string;
  cost: number;
  label: string;
}

interface ProviderCostOption {
  provider: string;
  model: string;
  costPer1kTokens: number;
  costPerHour: number;
  rating: number;
}

interface BudgetAlert {
  id: string;
  name: string;
  threshold: number;
  current: number;
  unit: string;
  enabled: boolean;
  triggered: boolean;
}

interface CostReport {
  generatedAt: string;
  period: string;
  totalCost: number;
  breakdown: CostBreakdownItem[];
  topModels: { name: string; cost: number; tokens: number }[];
  recommendations: string[];
}

// ============================================================================
// Constants
// ============================================================================

const MODELS = [
  "Llama-3.1-70B",
  "Mixtral-8x7B",
  "Qwen-2.5-72B",
  "DeepSeek-V3",
  "Phi-4-MoE",
  "Gemma-2-27B",
  "Mistral-Large",
  "Codestral-22B",
];

const PROVIDERS = [
  "AlphaNode",
  "BetaCompute",
  "GammaInfer",
  "DeltaGPU",
  "EpsilonNet",
  "ZetaML",
];

const DURATIONS = [
  { value: 1, label: "1 Hour" },
  { value: 24, label: "1 Day" },
  { value: 168, label: "1 Week" },
  { value: 720, label: "1 Month" },
];

const COST_COLORS = {
  compute: "#3b82f6",
  storage: "#8b5cf6",
  network: "#f59e0b",
  memory: "#10b981",
};

const MODEL_COST_MAP: Record<string, number> = {
  "Llama-3.1-70B": 0.008,
  "Mixtral-8x7B": 0.004,
  "Qwen-2.5-72B": 0.007,
  "DeepSeek-V3": 0.009,
  "Phi-4-MoE": 0.003,
  "Gemma-2-27B": 0.005,
  "Mistral-Large": 0.006,
  "Codestral-22B": 0.005,
};

// ============================================================================
// Helpers
// ============================================================================

function randomBetween(min: number, max: number): number {
  return Math.random() * (max - min) + min;
}

function formatCurrency(amount: number): string {
  if (amount >= 1000) return `$${(amount / 1000).toFixed(2)}K`;
  if (amount >= 1) return `$${amount.toFixed(2)}`;
  if (amount >= 0.01) return `$${amount.toFixed(3)}`;
  return `$${amount.toFixed(4)}`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function generateCostHistory(): CostHistoryPoint[] {
  const now = new Date();
  return Array.from({ length: 30 }, (_, i) => {
    const date = new Date(now);
    date.setDate(date.getDate() - (29 - i));
    return {
      date: date.toISOString().split("T")[0],
      cost: randomBetween(50, 250),
      label: date.toLocaleDateString("en-US", { month: "short", day: "numeric" }),
    };
  });
}

function generateProviderCosts(model: string): ProviderCostOption[] {
  const baseCost = MODEL_COST_MAP[model] || 0.005;
  return PROVIDERS.map((provider) => ({
    provider,
    model,
    costPer1kTokens: Math.round((baseCost * randomBetween(0.8, 1.4)) * 10000) / 10000,
    costPerHour: Math.round(baseCost * randomBetween(0.5, 2.0) * 100) / 100,
    rating: Math.round(randomBetween(3.5, 5.0) * 10) / 10,
  })).sort((a, b) => a.costPer1kTokens - b.costPer1kTokens);
}

function calculateBreakdown(totalCost: number): CostBreakdownItem[] {
  const compute = totalCost * randomBetween(0.55, 0.7);
  const storage = totalCost * randomBetween(0.1, 0.2);
  const network = totalCost * randomBetween(0.05, 0.15);
  const memory = totalCost - compute - storage - network;

  return [
    { category: "Compute", amount: compute, percentage: (compute / totalCost) * 100, color: COST_COLORS.compute },
    { category: "Storage", amount: storage, percentage: (storage / totalCost) * 100, color: COST_COLORS.storage },
    { category: "Network", amount: network, percentage: (network / totalCost) * 100, color: COST_COLORS.network },
    { category: "Memory", amount: memory, percentage: (memory / totalCost) * 100, color: COST_COLORS.memory },
  ];
}

// ============================================================================
// CostCalculator Component
// ============================================================================

function CostCalculator({
  selectedModel,
  setSelectedModel,
  tokens,
  setTokens,
  duration,
  setDuration,
  selectedProvider,
  setSelectedProvider,
  calculatedCost,
}: {
  selectedModel: string;
  setSelectedModel: (m: string) => void;
  tokens: number;
  setTokens: (t: number) => void;
  duration: number;
  setDuration: (d: number) => void;
  selectedProvider: string;
  setSelectedProvider: (p: string) => void;
  calculatedCost: number;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-1 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Cost Calculator
      </h3>
      <p className="mb-5 text-xs text-surface-500">
        Estimate your inference costs based on model, token count, and duration
      </p>

      <div className="space-y-4">
        {/* Model selector */}
        <div>
          <label className="mb-1.5 block text-xs font-medium text-surface-700 dark:text-surface-300">
            Model
          </label>
          <select
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-white px-3 py-2.5 text-sm text-surface-700 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>

        {/* Provider selector */}
        <div>
          <label className="mb-1.5 block text-xs font-medium text-surface-700 dark:text-surface-300">
            Provider
          </label>
          <select
            value={selectedProvider}
            onChange={(e) => setSelectedProvider(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-white px-3 py-2.5 text-sm text-surface-700 focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {PROVIDERS.map((p) => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
        </div>

        {/* Token count */}
        <div>
          <label className="mb-1.5 block text-xs font-medium text-surface-700 dark:text-surface-300">
            Token Count: <span className="text-surface-900 dark:text-surface-50">{formatNumber(tokens)}</span>
          </label>
          <input
            type="range"
            min={1000}
            max={10000000}
            step={1000}
            value={tokens}
            onChange={(e) => setTokens(Number(e.target.value))}
            className="w-full accent-blue-600"
          />
          <div className="mt-1 flex justify-between text-xs text-surface-400">
            <span>1K</span>
            <span>5M</span>
            <span>10M</span>
          </div>
        </div>

        {/* Duration */}
        <div>
          <label className="mb-1.5 block text-xs font-medium text-surface-700 dark:text-surface-300">
            Duration
          </label>
          <div className="flex gap-2">
            {DURATIONS.map((d) => (
              <button
                key={d.value}
                onClick={() => setDuration(d.value)}
                className={`flex-1 rounded-lg border px-3 py-2 text-xs font-medium transition-colors ${
                  duration === d.value
                    ? "border-blue-500 bg-blue-50 text-blue-700 dark:bg-blue-900/20 dark:text-blue-400"
                    : "border-surface-200 bg-white text-surface-600 hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-400"
                }`}
              >
                {d.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Calculated cost */}
      <div className="mt-5 rounded-lg bg-gradient-to-r from-blue-50 to-indigo-50 p-4 dark:from-blue-900/20 dark:to-indigo-900/20">
        <p className="text-xs text-surface-500">Estimated Cost</p>
        <p className="mt-1 text-3xl font-bold text-blue-700 dark:text-blue-400">
          {formatCurrency(calculatedCost)}
        </p>
        <p className="mt-1 text-xs text-surface-500">
          {formatCurrency(calculatedCost / duration)} per hour &middot; {formatCurrency((calculatedCost / tokens) * 1000)} per 1K tokens
        </p>
      </div>
    </div>
  );
}

// ============================================================================
// CostBreakdownChart Component - SVG Donut Chart
// ============================================================================

function CostBreakdownChart({ breakdown, totalCost }: { breakdown: CostBreakdownItem[]; totalCost: number }) {
  const size = 220;
  const strokeWidth = 40;
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const center = size / 2;

  let currentOffset = 0;
  const segments = breakdown.map((item) => {
    const segLength = (item.percentage / 100) * circumference;
    const seg = {
      ...item,
      offset: currentOffset,
      length: segLength,
    };
    currentOffset += segLength;
    return seg;
  });

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-4 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Cost Breakdown
      </h3>
      <div className="flex flex-col items-center sm:flex-row sm:items-start sm:gap-6">
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
          {/* Background circle */}
          <circle
            cx={center}
            cy={center}
            r={radius}
            fill="none"
            stroke="currentColor"
            className="text-surface-200 dark:text-surface-700"
            strokeWidth={strokeWidth}
          />
          {/* Segments */}
          {segments.map((seg, i) => (
            <circle
              key={i}
              cx={center}
              cy={center}
              r={radius}
              fill="none"
              stroke={seg.color}
              strokeWidth={strokeWidth}
              strokeDasharray={`${seg.length} ${circumference - seg.length}`}
              strokeDashoffset={-seg.offset}
              transform={`rotate(-90 ${center} ${center})`}
              className="transition-all duration-500"
            />
          ))}
          {/* Center text */}
          <text
            x={center}
            y={center - 6}
            textAnchor="middle"
            className="fill-surface-900 dark:fill-surface-50"
            fontSize={22}
            fontWeight="bold"
          >
            {formatCurrency(totalCost)}
          </text>
          <text
            x={center}
            y={center + 14}
            textAnchor="middle"
            className="fill-surface-500 dark:fill-surface-400"
            fontSize={11}
          >
            Total
          </text>
        </svg>

        {/* Legend */}
        <div className="mt-4 space-y-3 sm:mt-0">
          {breakdown.map((item) => (
            <div key={item.category} className="flex items-center gap-3">
              <span
                className="inline-block h-3 w-3 rounded-full"
                style={{ backgroundColor: item.color }}
              />
              <div>
                <p className="text-xs font-medium text-surface-700 dark:text-surface-300">
                  {item.category}
                </p>
                <p className="text-xs text-surface-500">
                  {formatCurrency(item.amount)} ({item.percentage.toFixed(1)}%)
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// CostTrendChart Component - SVG Line Chart
// ============================================================================

function CostTrendChart({ data }: { data: CostHistoryPoint[] }) {
  const width = 800;
  const height = 280;
  const padding = { top: 20, right: 20, bottom: 50, left: 60 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const values = data.map((d) => d.cost);
  const minVal = Math.max(0, Math.min(...values) - 20);
  const maxVal = Math.max(...values) + 20;
  const range = maxVal - minVal || 1;

  const points = data.map((d, i) => ({
    x: padding.left + (i / Math.max(data.length - 1, 1)) * chartW,
    y: padding.top + chartH - ((d.cost - minVal) / range) * chartH,
  }));

  const linePath =
    points.length > 0
      ? points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ")
      : "";

  const areaPath =
    points.length > 0
      ? `${linePath} L ${points[points.length - 1].x} ${padding.top + chartH} L ${points[0].x} ${padding.top + chartH} Z`
      : "";

  // Average line
  const avgCost = values.reduce((sum, v) => sum + v, 0) / values.length;
  const avgY = padding.top + chartH - ((avgCost - minVal) / range) * chartH;

  // Show every 5th label
  const labelStep = Math.max(Math.floor(data.length / 6), 1);

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Historical Cost Trend
        </h3>
        <span className="text-xs text-surface-500">
          Avg: {formatCurrency(avgCost)}/day
        </span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <linearGradient id="costGradient" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#8b5cf6" stopOpacity="0.25" />
            <stop offset="100%" stopColor="#8b5cf6" stopOpacity="0.02" />
          </linearGradient>
        </defs>

        {/* Grid lines */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac, i) => {
          const y = padding.top + chartH * (1 - frac);
          const val = minVal + range * frac;
          return (
            <g key={`cgrid-${i}`}>
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
                ${Math.round(val)}
              </text>
            </g>
          );
        })}

        {/* Average line */}
        <line
          x1={padding.left}
          y1={avgY}
          x2={width - padding.right}
          y2={avgY}
          stroke="#8b5cf6"
          strokeWidth={1.5}
          strokeDasharray="8 4"
          opacity={0.5}
        />
        <text
          x={width - padding.right + 4}
          y={avgY - 4}
          className="fill-surface-400"
          fontSize={9}
        >
          avg
        </text>

        {/* Area fill */}
        {areaPath && <path d={areaPath} fill="url(#costGradient)" />}

        {/* Line */}
        {linePath && (
          <path
            d={linePath}
            fill="none"
            stroke="#8b5cf6"
            strokeWidth={2.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        )}

        {/* Data points */}
        {points.map((p, i) => {
          if (i % labelStep !== 0 && i !== points.length - 1) return null;
          return (
            <g key={`dot-${i}`}>
              <circle cx={p.x} cy={p.y} r={3} fill="#8b5cf6" stroke="white" strokeWidth={1.5} />
              <text
                x={p.x}
                y={height - 8}
                textAnchor="middle"
                className="fill-surface-400 dark:fill-surface-500"
                fontSize={9}
              >
                {data[i].label}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ============================================================================
// CostComparison Component - SVG Bar Chart
// ============================================================================>

function CostComparisonChart({ options }: { options: ProviderCostOption[] }) {
  const width = 800;
  const height = 320;
  const padding = { top: 20, right: 20, bottom: 60, left: 80 };
  const chartW = width - padding.left - padding.right;
  const chartH = height - padding.top - padding.bottom;

  const maxCost = Math.max(...options.map((o) => o.costPer1kTokens), 0.001);
  const barHeight = Math.min(chartW / options.length - 16, 60);

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Provider Cost Comparison
        </h3>
        <span className="text-xs text-surface-500">Cost per 1K tokens</span>
      </div>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Grid lines */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac, i) => {
          const x = padding.left + chartW * frac;
          const val = maxCost * frac;
          return (
            <g key={`ccgrid-${i}`}>
              <line
                x1={x}
                y1={padding.top}
                x2={x}
                y2={height - padding.bottom}
                stroke="currentColor"
                className="text-surface-200 dark:text-surface-700"
                strokeWidth={1}
                strokeDasharray="4 4"
              />
              <text
                x={x}
                y={height - 8}
                textAnchor="middle"
                className="fill-surface-400 dark:fill-surface-500"
                fontSize={10}
              >
                ${val.toFixed(4)}
              </text>
            </g>
          );
        })}

        {/* Bars */}
        {options.map((option, i) => {
          const barW = (option.costPer1kTokens / maxCost) * chartW;
          const y = padding.top + (i * chartH) / options.length + 8;
          const isCheapest = i === 0;

          return (
            <g key={option.provider}>
              <text
                x={padding.left - 8}
                y={y + barHeight / 2 + 4}
                textAnchor="end"
                className={`fill-surface-700 dark:fill-surface-300 ${isCheapest ? "font-bold" : ""}`}
                fontSize={12}
              >
                {option.provider}
              </text>
              <rect
                x={padding.left}
                y={y}
                width={barW}
                height={barHeight}
                fill={isCheapest ? "#10b981" : "#3b82f6"}
                rx={4}
                ry={4}
                opacity={0.85}
              />
              <text
                x={padding.left + barW + 8}
                y={y + barHeight / 2 + 4}
                className="fill-surface-700 dark:fill-surface-300"
                fontSize={11}
                fontWeight="500"
              >
                ${option.costPer1kTokens.toFixed(4)}
              </text>
              {isCheapest && (
                <text
                  x={padding.left + 6}
                  y={y + barHeight / 2 + 4}
                  className="fill-white"
                  fontSize={10}
                  fontWeight="bold"
                >
                  BEST
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ============================================================================
// BudgetAlerts Component
// ============================================================================

function BudgetAlerts({
  alerts,
  onToggle,
  onUpdateThreshold,
}: {
  alerts: BudgetAlert[];
  onToggle: (id: string) => void;
  onUpdateThreshold: (id: string, threshold: number) => void;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-1 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Budget Alerts
      </h3>
      <p className="mb-4 text-xs text-surface-500">
        Configure spending alerts to stay within budget
      </p>
      <div className="space-y-3">
        {alerts.map((alert) => (
          <div
            key={alert.id}
            className={`rounded-lg border p-4 transition-colors ${
              alert.triggered
                ? "border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20"
                : "border-surface-200 bg-surface-50 dark:border-surface-700 dark:bg-surface-800/50"
            }`}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <label className="relative inline-flex cursor-pointer items-center">
                  <input
                    type="checkbox"
                    checked={alert.enabled}
                    onChange={() => onToggle(alert.id)}
                    className="peer sr-only"
                  />
                  <div className="h-5 w-9 rounded-full bg-surface-300 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-blue-600 peer-checked:after:translate-x-full dark:bg-surface-600" />
                </label>
                <div>
                  <p className="text-sm font-medium text-surface-900 dark:text-surface-50">
                    {alert.name}
                  </p>
                  {alert.triggered && (
                    <p className="text-xs font-medium text-red-600 dark:text-red-400">
                      &#x26A0; Alert triggered!
                    </p>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-xs text-surface-500">
                  {alert.unit === "$" ? formatCurrency(alert.threshold) : `${alert.threshold} ${alert.unit}`}
                </span>
              </div>
            </div>
            <div className="mt-3">
              <input
                type="range"
                min={0}
                max={alert.unit === "$" ? 1000 : 100}
                step={alert.unit === "$" ? 10 : 1}
                value={alert.threshold}
                onChange={(e) => onUpdateThreshold(alert.id, Number(e.target.value))}
                disabled={!alert.enabled}
                className="w-full accent-blue-600 disabled:opacity-50"
              />
              <div className="mt-1 flex justify-between text-xs text-surface-400">
                <span>Current: {alert.unit === "$" ? formatCurrency(alert.current) : `${alert.current} ${alert.unit}`}</span>
                <span>Threshold: {alert.unit === "$" ? formatCurrency(alert.threshold) : `${alert.threshold} ${alert.unit}`}</span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// CostReport Component
// ============================================================================

function CostReport({
  report,
  onExport,
}: {
  report: CostReport | null;
  onExport: () => void;
}) {
  if (!report) {
    return (
      <div className="rounded-xl border border-surface-200 bg-white p-6 shadow-sm dark:border-surface-800 dark:bg-surface-900">
        <h3 className="mb-1 text-sm font-semibold text-surface-900 dark:text-surface-50">
          Cost Report
        </h3>
        <p className="mb-4 text-xs text-surface-500">
          Generate a detailed cost report for your inference usage
        </p>
        <button
          onClick={onExport}
          className="w-full rounded-lg bg-blue-600 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-blue-700"
        >
          Generate Report
        </button>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Cost Report
        </h3>
        <span className="text-xs text-surface-400">
          {report.generatedAt}
        </span>
      </div>

      <div className="mb-4 rounded-lg bg-gradient-to-r from-indigo-50 to-purple-50 p-4 dark:from-indigo-900/20 dark:to-purple-900/20">
        <p className="text-xs text-surface-500">Period Total</p>
        <p className="text-2xl font-bold text-indigo-700 dark:text-indigo-400">
          {formatCurrency(report.totalCost)}
        </p>
        <p className="text-xs text-surface-500">{report.period}</p>
      </div>

      {/* Top models */}
      <div className="mb-4">
        <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-surface-500">
          Top Models by Cost
        </h4>
        <div className="space-y-2">
          {report.topModels.map((model, i) => (
            <div key={i} className="flex items-center justify-between rounded-lg bg-surface-50 px-3 py-2 dark:bg-surface-800/50">
              <div>
                <span className="text-xs font-medium text-surface-900 dark:text-surface-50">
                  {i + 1}. {model.name}
                </span>
                <p className="text-xs text-surface-400">{formatNumber(model.tokens)} tokens</p>
              </div>
              <span className="text-sm font-semibold text-surface-900 dark:text-surface-50">
                {formatCurrency(model.cost)}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Recommendations */}
      <div className="mb-4">
        <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-surface-500">
          Recommendations
        </h4>
        <ul className="space-y-1">
          {report.recommendations.map((rec, i) => (
            <li key={i} className="flex items-start gap-2 text-xs text-surface-600 dark:text-surface-400">
              <span className="mt-0.5 text-emerald-500">&#x2713;</span>
              {rec}
            </li>
          ))}
        </ul>
      </div>

      <button
        onClick={onExport}
        className="w-full rounded-lg border border-surface-200 bg-white px-4 py-2 text-xs font-medium text-surface-700 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
      >
        Export Report (CSV)
      </button>
    </div>
  );
}

// ============================================================================
// ProjectionSummary Component
// ============================================================================

function ProjectionSummary({
  monthlyCost,
  yearlyCost,
  savings,
}: {
  monthlyCost: number;
  yearlyCost: number;
  savings: number;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
        <p className="text-xs font-medium uppercase tracking-wider text-surface-500">Monthly Projection</p>
        <p className="mt-2 text-2xl font-bold text-surface-900 dark:text-surface-50">
          {formatCurrency(monthlyCost)}
        </p>
        <p className="mt-1 text-xs text-surface-400">Based on current usage patterns</p>
      </div>
      <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
        <p className="text-xs font-medium uppercase tracking-wider text-surface-500">Annual Projection</p>
        <p className="mt-2 text-2xl font-bold text-surface-900 dark:text-surface-50">
          {formatCurrency(yearlyCost)}
        </p>
        <p className="mt-1 text-xs text-surface-400">Estimated yearly spend</p>
      </div>
      <div className="rounded-xl border border-emerald-200 bg-emerald-50 p-5 shadow-sm dark:border-emerald-800 dark:bg-emerald-900/20">
        <p className="text-xs font-medium uppercase tracking-wider text-emerald-600 dark:text-emerald-400">
          Potential Savings
        </p>
        <p className="mt-2 text-2xl font-bold text-emerald-700 dark:text-emerald-400">
          {formatCurrency(savings)}
        </p>
        <p className="mt-1 text-xs text-emerald-600 dark:text-emerald-400">By switching to optimal provider</p>
      </div>
    </div>
  );
}

// ============================================================================
// Main Page Component
// ============================================================================

export default function CostsPage() {
  const [selectedModel, setSelectedModel] = useState(MODELS[0]);
  const [tokens, setTokens] = useState(1000000);
  const [duration, setDuration] = useState(24);
  const [selectedProvider, setSelectedProvider] = useState(PROVIDERS[0]);
  const [costHistory] = useState<CostHistoryPoint[]>(() => generateCostHistory());
  const [report, setReport] = useState<CostReport | null>(null);

  // Budget alerts state
  const [alerts, setAlerts] = useState<BudgetAlert[]>([
    { id: "1", name: "Daily Spend Limit", threshold: 100, current: 78.50, unit: "$", enabled: true, triggered: false },
    { id: "2", name: "Weekly Spend Limit", threshold: 500, current: 423.80, unit: "$", enabled: true, triggered: false },
    { id: "3", name: "Token Usage Alert", threshold: 80, current: 65, unit: "%", enabled: true, triggered: false },
    { id: "4", name: "Monthly Budget", threshold: 800, current: 612.30, unit: "$", enabled: false, triggered: false },
  ]);

  // Calculate cost
  const calculatedCost = useMemo(() => {
    const costPer1k = MODEL_COST_MAP[selectedModel] || 0.005;
    const tokenCost = (tokens / 1000) * costPer1k;
    const hourlyCost = costPer1k * 500 * (duration > 24 ? 0.85 : 1);
    return Math.round((tokenCost + hourlyCost) * 100) / 100;
  }, [selectedModel, tokens, duration]);

  // Cost breakdown
  const breakdown = useMemo(() => calculateBreakdown(calculatedCost), [calculatedCost]);

  // Provider comparison
  const providerCosts = useMemo(
    () => generateProviderCosts(selectedModel),
    [selectedModel]
  );

  // Projections
  const projections = useMemo(() => {
    const dailyAvg = costHistory.reduce((sum, d) => sum + d.cost, 0) / costHistory.length;
    const monthly = dailyAvg * 30;
    const yearly = dailyAvg * 365;
    const cheapestProvider = providerCosts[0]?.costPer1kTokens || 0.005;
    const currentAvgCost = Object.values(MODEL_COST_MAP).reduce((a, b) => a + b, 0) / MODELS.length;
    const savings = yearly * (1 - cheapestProvider / currentAvgCost);
    return { monthly, yearly, savings };
  }, [costHistory, providerCosts]);

  // Alert handlers
  const handleToggleAlert = useCallback((id: string) => {
    setAlerts((prev) =>
      prev.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a))
    );
  }, []);

  const handleUpdateThreshold = useCallback((id: string, threshold: number) => {
    setAlerts((prev) =>
      prev.map((a) => (a.id === id ? { ...a, threshold, triggered: a.current >= threshold } : a))
    );
  }, []);

  // Generate report
  const handleGenerateReport = useCallback(() => {
    const topModels = MODELS.slice(0, 5).map((name) => ({
      name,
      cost: Math.round(randomBetween(20, 200) * 100) / 100,
      tokens: Math.floor(randomBetween(50000, 2000000)),
    })).sort((a, b) => b.cost - a.cost);

    const recommendations = [
      "Switch from DeltaGPU to AlphaNode for Llama-3.1-70B to save ~15% on compute costs",
      "Consider batching requests during off-peak hours for 10-20% cost reduction",
      "Enable response caching for repeated queries to reduce token usage by ~25%",
      "Migrate low-priority workloads to Phi-4-MoE for significant cost savings",
    ];

    setReport({
      generatedAt: new Date().toLocaleString(),
      period: `Last 30 days (${costHistory[0]?.date} - ${costHistory[costHistory.length - 1]?.date})`,
      totalCost: costHistory.reduce((sum, d) => sum + d.cost, 0),
      breakdown,
      topModels,
      recommendations,
    });
  }, [costHistory, breakdown]);

  // Export handler
  const handleExport = useCallback(() => {
    handleGenerateReport();
    // Simulate CSV export
    const blob = new Blob(["Cost Report Export - Simulation"], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "cost-report.csv";
    a.click();
    URL.revokeObjectURL(url);
  }, [handleGenerateReport]);

  return (
    <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-50">
            Cost Projections
          </h1>
          <p className="mt-1 text-sm text-surface-600 dark:text-surface-400">
            Estimate and optimize your inference spending across providers and models
          </p>
        </div>
      </div>

      {/* Projection Summary */}
      <div className="mt-8">
        <ProjectionSummary
          monthlyCost={projections.monthly}
          yearlyCost={projections.yearly}
          savings={projections.savings}
        />
      </div>

      {/* Calculator + Breakdown */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
        <CostCalculator
          selectedModel={selectedModel}
          setSelectedModel={setSelectedModel}
          tokens={tokens}
          setTokens={setTokens}
          duration={duration}
          setDuration={setDuration}
          selectedProvider={selectedProvider}
          setSelectedProvider={setSelectedProvider}
          calculatedCost={calculatedCost}
        />
        <CostBreakdownChart breakdown={breakdown} totalCost={calculatedCost} />
      </div>

      {/* Historical Trend */}
      <div className="mt-6">
        <CostTrendChart data={costHistory} />
      </div>

      {/* Provider Comparison + Budget Alerts + Report */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <CostComparisonChart options={providerCosts} />
        </div>
        <div className="space-y-6">
          <CostReport report={report} onExport={handleExport} />
        </div>
      </div>

      {/* Budget Alerts */}
      <div className="mt-6">
        <BudgetAlerts
          alerts={alerts}
          onToggle={handleToggleAlert}
          onUpdateThreshold={handleUpdateThreshold}
        />
      </div>

      {/* Footer */}
      <div className="mt-8 rounded-lg bg-surface-50 px-4 py-3 dark:bg-surface-800/50">
        <p className="text-xs text-surface-500">
          Cost projections are estimates based on current pricing and usage patterns.
          Actual costs may vary based on provider pricing changes, model updates, and usage patterns.
          All data is simulated for demonstration purposes.
        </p>
      </div>
    </div>
  );
}
