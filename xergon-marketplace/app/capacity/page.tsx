"use client";

import { useState, useEffect, useCallback, useMemo } from "react";

// ============================================================================
// Types
// ============================================================================

interface ProviderNode {
  id: string;
  name: string;
  region: string;
  gpuType: string;
  vramGB: number;
  utilization: number; // 0-100
  status: "online" | "offline" | "maintenance";
  currentLoad: number;
  maxLoad: number;
  availableModels: string[];
  temperature: number; // celsius
  uptime: number; // hours
  totalRequests: number;
  errorRate: number;
}

interface FilterState {
  gpuType: string;
  region: string;
  status: string;
}

interface CapacitySummary {
  totalProviders: number;
  onlineProviders: number;
  averageUtilization: number;
  availableCapacity: number;
  totalVRAM: number;
  usedVRAM: number;
}

// ============================================================================
// Constants
// ============================================================================

const GPU_TYPES = ["H100 80GB", "H100 40GB", "A100 80GB", "A100 40GB", "L40S 48GB", "RTX 4090 24GB"];
const REGIONS = ["US-East", "US-West", "EU-West", "EU-Central", "APAC-Tokyo", "APAC-Seoul"];
const MODEL_OPTIONS = ["Llama-3.1-70B", "Mixtral-8x7B", "Qwen-2.5-72B", "DeepSeek-V3", "Phi-4-MoE", "Gemma-2-27B", "Mistral-Large", "Codestral-22B"];

const PROVIDER_NAMES = [
  "AlphaNode", "BetaCompute", "GammaInfer", "DeltaGPU", "EpsilonNet",
  "ZetaML", "EtaServe", "ThetaCloud", "IotaHost", "KappaRun",
  "LambdaOps", "MuTensor", "NuCore", "XiAccelerate", "OmicronAI",
  "PiCompute", "RhoFlux", "SigmaNode", "TauEngine", "UpsilonML",
];

// ============================================================================
// Mock Data Generators
// ============================================================================

function randomBetween(min: number, max: number): number {
  return Math.random() * (max - min) + min;
}

function randomPick<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

function randomSubset<T>(arr: T[], minCount: number): T[] {
  const shuffled = [...arr].sort(() => Math.random() - 0.5);
  return shuffled.slice(0, Math.floor(randomBetween(minCount, arr.length)));
}

function generateProviders(): ProviderNode[] {
  return PROVIDER_NAMES.map((name, i) => {
    const statusRoll = Math.random();
    let status: ProviderNode["status"] = "online";
    if (statusRoll > 0.9) status = "offline";
    else if (statusRoll > 0.82) status = "maintenance";

    const utilization = status === "online" ? randomBetween(5, 95) : 0;
    const gpuType = randomPick(GPU_TYPES);
    const vramMatch = gpuType.match(/(\d+)GB/);
    const vramGB = vramMatch ? parseInt(vramMatch[1]) : 80;
    const maxLoad = Math.floor(randomBetween(50, 200));

    return {
      id: `provider-${i + 1}`,
      name,
      region: randomPick(REGIONS),
      gpuType,
      vramGB,
      utilization: Math.round(utilization * 10) / 10,
      status,
      currentLoad: status === "online" ? Math.floor(maxLoad * (utilization / 100)) : 0,
      maxLoad,
      availableModels: status === "online" ? randomSubset(MODEL_OPTIONS, 2) : [],
      temperature: status === "online" ? Math.round(randomBetween(35, 85)) : 0,
      uptime: status === "online" ? Math.floor(randomBetween(1, 720)) : 0,
      totalRequests: status === "online" ? Math.floor(randomBetween(1000, 500000)) : 0,
      errorRate: status === "online" ? Math.round(randomBetween(0, 5) * 100) / 100 : 0,
    };
  });
}

// ============================================================================
// Helpers
// ============================================================================

function getUtilizationColor(utilization: number, status: ProviderNode["status"]): string {
  if (status === "offline") return "bg-gray-400 dark:bg-gray-600";
  if (status === "maintenance") return "bg-surface-400 dark:bg-surface-600";
  if (utilization < 40) return "bg-emerald-500";
  if (utilization < 70) return "bg-amber-400";
  return "bg-red-500";
}

function getUtilizationTextColor(utilization: number, status: ProviderNode["status"]): string {
  if (status === "offline") return "text-gray-500";
  if (status === "maintenance") return "text-surface-500";
  if (utilization < 40) return "text-emerald-600 dark:text-emerald-400";
  if (utilization < 70) return "text-amber-600 dark:text-amber-400";
  return "text-red-600 dark:text-red-400";
}

function getUtilizationBgColor(utilization: number, status: ProviderNode["status"]): string {
  if (status === "offline") return "bg-gray-100 dark:bg-gray-800/50";
  if (status === "maintenance") return "bg-surface-100 dark:bg-surface-800/50";
  if (utilization < 40) return "bg-emerald-50 dark:bg-emerald-900/20";
  if (utilization < 70) return "bg-amber-50 dark:bg-amber-900/20";
  return "bg-red-50 dark:bg-red-900/20";
}

function getUtilizationBorderColor(utilization: number, status: ProviderNode["status"]): string {
  if (status === "offline") return "border-gray-200 dark:border-gray-700";
  if (status === "maintenance") return "border-surface-200 dark:border-surface-700";
  if (utilization < 40) return "border-emerald-200 dark:border-emerald-800";
  if (utilization < 70) return "border-amber-200 dark:border-amber-800";
  return "border-red-200 dark:border-red-800";
}

function formatUptime(hours: number): string {
  if (hours >= 24) return `${(hours / 24).toFixed(1)}d`;
  return `${hours}h`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

// ============================================================================
// CapacityFilters Component
// ============================================================================

function CapacityFilters({
  filters,
  onChange,
}: {
  filters: FilterState;
  onChange: (filters: FilterState) => void;
}) {
  const allGpuTypes = ["All", ...GPU_TYPES];
  const allRegions = ["All", ...REGIONS];
  const allStatuses = ["All", "online", "offline", "maintenance"];

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-4 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-3 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Filters
      </h3>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div>
          <label className="mb-1 block text-xs font-medium text-surface-500">GPU Type</label>
          <select
            value={filters.gpuType}
            onChange={(e) => onChange({ ...filters, gpuType: e.target.value })}
            className="w-full rounded-lg border border-surface-200 bg-white px-3 py-2 text-sm text-surface-700 transition-colors focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {allGpuTypes.map((g) => (
              <option key={g} value={g === "All" ? "" : g}>{g}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="mb-1 block text-xs font-medium text-surface-500">Region</label>
          <select
            value={filters.region}
            onChange={(e) => onChange({ ...filters, region: e.target.value })}
            className="w-full rounded-lg border border-surface-200 bg-white px-3 py-2 text-sm text-surface-700 transition-colors focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {allRegions.map((r) => (
              <option key={r} value={r === "All" ? "" : r}>{r}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="mb-1 block text-xs font-medium text-surface-500">Status</label>
          <select
            value={filters.status}
            onChange={(e) => onChange({ ...filters, status: e.target.value })}
            className="w-full rounded-lg border border-surface-200 bg-white px-3 py-2 text-sm text-surface-700 transition-colors focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300"
          >
            {allStatuses.map((s) => (
              <option key={s} value={s === "All" ? "" : s}>{s.charAt(0).toUpperCase() + s.slice(1)}</option>
            ))}
          </select>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// CapacitySummary Component
// ============================================================================

function CapacitySummaryCard({
  label,
  value,
  subtitle,
  icon,
}: {
  label: string;
  value: string;
  subtitle?: string;
  icon: string;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-white p-4 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="flex items-center gap-3">
        <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-surface-100 text-lg dark:bg-surface-800">
          {icon}
        </div>
        <div>
          <p className="text-xs font-medium text-surface-500">{label}</p>
          <p className="text-lg font-bold text-surface-900 dark:text-surface-50">{value}</p>
          {subtitle && <p className="text-xs text-surface-400">{subtitle}</p>}
        </div>
      </div>
    </div>
  );
}

function CapacitySummaryStats({ providers }: { providers: ProviderNode[] }) {
  const online = providers.filter((p) => p.status === "online");
  const avgUtil = online.length > 0
    ? online.reduce((sum, p) => sum + p.utilization, 0) / online.length
    : 0;
  const totalVRAM = providers.reduce((sum, p) => sum + p.vramGB, 0);
  const usedVRAM = online.reduce((sum, p) => sum + p.vramGB * (p.utilization / 100), 0);
  const available = totalVRAM - usedVRAM;

  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <CapacitySummaryCard
        label="Total Providers"
        value={`${online.length}/${providers.length}`}
        subtitle={`${providers.length - online.length} offline/maintenance`}
        icon="&#x1F5A5;"
      />
      <CapacitySummaryCard
        label="Avg Utilization"
        value={`${avgUtil.toFixed(1)}%`}
        subtitle={avgUtil < 50 ? "Healthy load" : avgUtil < 75 ? "Moderate load" : "High load"}
        icon="&#x1F4C8;"
      />
      <CapacitySummaryCard
        label="Total VRAM"
        value={`${formatNumber(totalVRAM)} GB`}
        subtitle={`${formatNumber(usedVRAM)} GB in use`}
        icon="&#x1F9E0;"
      />
      <CapacitySummaryCard
        label="Available"
        value={`${formatNumber(available)} GB`}
        subtitle={`${((available / totalVRAM) * 100).toFixed(1)}% free`}
        icon="&#x2705;"
      />
    </div>
  );
}

// ============================================================================
// ProviderCard Component (Heatmap Cell)
// ============================================================================

function ProviderCard({
  provider,
  isSelected,
  onClick,
}: {
  provider: ProviderNode;
  isSelected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`relative flex flex-col items-center justify-center rounded-xl border-2 p-4 transition-all hover:scale-[1.02] hover:shadow-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 ${getUtilizationBgColor(provider.utilization, provider.status)} ${getUtilizationBorderColor(provider.utilization, provider.status)} ${isSelected ? "ring-2 ring-blue-500 ring-offset-2 dark:ring-offset-surface-900" : ""}`}
      aria-label={`${provider.name}: ${provider.status}, ${provider.utilization}% utilization`}
    >
      {/* Status indicator */}
      <div className={`absolute top-2 right-2 h-2.5 w-2.5 rounded-full ${getUtilizationColor(provider.utilization, provider.status)}`} />

      {/* Provider name */}
      <p className="text-sm font-semibold text-surface-900 dark:text-surface-50">
        {provider.name}
      </p>

      {/* Region */}
      <p className="mt-0.5 text-xs text-surface-500">{provider.region}</p>

      {/* GPU type */}
      <p className="mt-1 text-xs font-medium text-surface-700 dark:text-surface-400">
        {provider.gpuType}
      </p>

      {/* Utilization bar */}
      <div className="mt-2 w-full">
        <div className="h-2 w-full overflow-hidden rounded-full bg-surface-200 dark:bg-surface-700">
          <div
            className={`h-full rounded-full transition-all duration-500 ${getUtilizationColor(provider.utilization, provider.status)}`}
            style={{ width: `${provider.utilization}%` }}
          />
        </div>
        <p className={`mt-1 text-center text-xs font-bold ${getUtilizationTextColor(provider.utilization, provider.status)}`}>
          {provider.status === "online" ? `${provider.utilization}%` : provider.status.toUpperCase()}
        </p>
      </div>
    </button>
  );
}

// ============================================================================
// ProviderDetail Component
// ============================================================================

function ProviderDetail({ provider, onClose }: { provider: ProviderNode; onClose: () => void }) {
  if (!provider) return null;

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-6 shadow-lg dark:border-surface-800 dark:bg-surface-900">
      <div className="flex items-start justify-between">
        <div>
          <h3 className="text-lg font-bold text-surface-900 dark:text-surface-50">
            {provider.name}
          </h3>
          <p className="text-sm text-surface-500">{provider.id}</p>
        </div>
        <button
          onClick={onClose}
          className="rounded-lg p-1.5 text-surface-400 transition-colors hover:bg-surface-100 hover:text-surface-600 dark:hover:bg-surface-800 dark:hover:text-surface-300"
          aria-label="Close detail panel"
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M6 6l8 8M14 6l-8 8" />
          </svg>
        </button>
      </div>

      {/* Status */}
      <div className="mt-4 flex items-center gap-2">
        <span className={`inline-block h-3 w-3 rounded-full ${getUtilizationColor(provider.utilization, provider.status)}`} />
        <span className={`text-sm font-medium capitalize ${getUtilizationTextColor(provider.utilization, provider.status)}`}>
          {provider.status}
        </span>
      </div>

      {/* Stats grid */}
      <div className="mt-5 grid grid-cols-2 gap-4">
        <StatItem label="GPU Type" value={provider.gpuType} />
        <StatItem label="VRAM" value={`${provider.vramGB} GB`} />
        <StatItem label="Utilization" value={`${provider.utilization}%`} highlight />
        <StatItem label="Temperature" value={`${provider.temperature}°C`} />
        <StatItem label="Current Load" value={`${provider.currentLoad} req/s`} />
        <StatItem label="Max Load" value={`${provider.maxLoad} req/s`} />
        <StatItem label="Uptime" value={formatUptime(provider.uptime)} />
        <StatItem label="Total Requests" value={formatNumber(provider.totalRequests)} />
        <StatItem label="Error Rate" value={`${provider.errorRate}%`} />
        <StatItem label="Region" value={provider.region} />
      </div>

      {/* Available Models */}
      <div className="mt-5">
        <h4 className="mb-2 text-sm font-semibold text-surface-900 dark:text-surface-50">
          Available Models ({provider.availableModels.length})
        </h4>
        <div className="flex flex-wrap gap-2">
          {provider.availableModels.map((model) => (
            <span
              key={model}
              className="inline-flex items-center rounded-full bg-surface-100 px-3 py-1 text-xs font-medium text-surface-700 dark:bg-surface-800 dark:text-surface-300"
            >
              {model}
            </span>
          ))}
        </div>
      </div>

      {/* Utilization bar */}
      <div className="mt-5">
        <div className="mb-1 flex items-center justify-between text-xs text-surface-500">
          <span>Capacity Used</span>
          <span className="font-medium">{provider.utilization}%</span>
        </div>
        <div className="h-3 w-full overflow-hidden rounded-full bg-surface-200 dark:bg-surface-700">
          <div
            className={`h-full rounded-full transition-all duration-700 ${getUtilizationColor(provider.utilization, provider.status)}`}
            style={{ width: `${provider.utilization}%` }}
          />
        </div>
      </div>
    </div>
  );
}

function StatItem({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="rounded-lg bg-surface-50 p-3 dark:bg-surface-800/50">
      <p className="text-xs text-surface-500">{label}</p>
      <p className={`mt-0.5 text-sm font-semibold ${highlight ? getUtilizationTextColor(parseFloat(value), "online") : "text-surface-900 dark:text-surface-50"}`}>
        {value}
      </p>
    </div>
  );
}

// ============================================================================
// CapacityGrid Component
// ============================================================================

function CapacityGrid({
  providers,
  selectedId,
  onSelect,
}: {
  providers: ProviderNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  // Sort by utilization descending
  const sorted = [...providers].sort((a, b) => {
    if (a.status === "offline" && b.status !== "offline") return 1;
    if (a.status !== "offline" && b.status === "offline") return -1;
    return b.utilization - a.utilization;
  });

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-50">
          Provider Capacity Heatmap
        </h3>
        <span className="text-xs text-surface-500">{sorted.length} nodes</span>
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
        {sorted.map((provider) => (
          <ProviderCard
            key={provider.id}
            provider={provider}
            isSelected={selectedId === provider.id}
            onClick={() => onSelect(provider.id)}
          />
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// Legend Component
// ============================================================================

function Legend() {
  const items = [
    { color: "bg-emerald-500", label: "Low (<40%)", desc: "Available capacity" },
    { color: "bg-amber-400", label: "Medium (40-70%)", desc: "Moderate load" },
    { color: "bg-red-500", label: "High (>70%)", desc: "Near capacity" },
    { color: "bg-gray-400", label: "Offline", desc: "Not available" },
  ];

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-4 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-3 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Legend
      </h3>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {items.map((item) => (
          <div key={item.label} className="flex items-center gap-2">
            <span className={`inline-block h-3 w-3 rounded ${item.color}`} />
            <div>
              <p className="text-xs font-medium text-surface-700 dark:text-surface-300">{item.label}</p>
              <p className="text-xs text-surface-400">{item.desc}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// RegionDistribution Component
// ============================================================================

function RegionDistribution({ providers }: { providers: ProviderNode[] }) {
  const regionCounts = REGIONS.map((region) => {
    const nodes = providers.filter((p) => p.region === region);
    const online = nodes.filter((n) => n.status === "online");
    const avgUtil = online.length > 0
      ? online.reduce((sum, p) => sum + p.utilization, 0) / online.length
      : 0;
    return { region, total: nodes.length, online: online.length, avgUtil };
  }).filter((r) => r.total > 0);

  return (
    <div className="rounded-xl border border-surface-200 bg-white p-5 shadow-sm dark:border-surface-800 dark:bg-surface-900">
      <h3 className="mb-4 text-sm font-semibold text-surface-900 dark:text-surface-50">
        Regional Distribution
      </h3>
      <div className="space-y-3">
        {regionCounts.map(({ region, total, online, avgUtil }) => (
          <div key={region} className="flex items-center gap-3">
            <span className="w-24 text-xs font-medium text-surface-700 dark:text-surface-300">{region}</span>
            <div className="flex-1">
              <div className="h-3 w-full overflow-hidden rounded-full bg-surface-200 dark:bg-surface-700">
                <div
                  className={`h-full rounded-full transition-all duration-500 ${
                    avgUtil < 40 ? "bg-emerald-500" : avgUtil < 70 ? "bg-amber-400" : "bg-red-500"
                  }`}
                  style={{ width: `${avgUtil}%` }}
                />
              </div>
            </div>
            <span className="w-12 text-right text-xs text-surface-500">{avgUtil.toFixed(0)}%</span>
            <span className="w-16 text-right text-xs text-surface-400">{online}/{total}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// Main Page Component
// ============================================================================

export default function CapacityPage() {
  const [providers, setProviders] = useState<ProviderNode[]>(() => generateProviders());
  const [filters, setFilters] = useState<FilterState>({ gpuType: "", region: "", status: "" });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState(Date.now());
  const [autoRefresh, setAutoRefresh] = useState(true);

  const refreshData = useCallback(() => {
    setProviders(generateProviders());
    setLastRefresh(Date.now());
  }, []);

  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(refreshData, 5000);
    return () => clearInterval(interval);
  }, [autoRefresh, refreshData]);

  // Filter providers
  const filteredProviders = useMemo(() => {
    return providers.filter((p) => {
      if (filters.gpuType && p.gpuType !== filters.gpuType) return false;
      if (filters.region && p.region !== filters.region) return false;
      if (filters.status && p.status !== filters.status) return false;
      return true;
    });
  }, [providers, filters]);

  const selectedProvider = useMemo(
    () => providers.find((p) => p.id === selectedId) ?? null,
    [providers, selectedId]
  );

  const activeFiltersCount = [filters.gpuType, filters.region, filters.status].filter(Boolean).length;

  return (
    <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-50">
            Provider Capacity
          </h1>
          <p className="mt-1 text-sm text-surface-600 dark:text-surface-400">
            GPU capacity heatmap across all inference providers
          </p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={refreshData}
            className="rounded-lg border border-surface-200 bg-white px-3 py-1.5 text-xs font-medium text-surface-700 transition-colors hover:bg-surface-50 dark:border-surface-700 dark:bg-surface-800 dark:text-surface-300 dark:hover:bg-surface-700"
          >
            Refresh
          </button>
          <label className="flex items-center gap-2 text-xs text-surface-500">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded border-surface-300 text-blue-600 focus:ring-blue-500"
            />
            Auto-refresh (5s)
          </label>
          <span className="text-xs text-surface-400">
            Updated {new Date(lastRefresh).toLocaleTimeString()}
          </span>
        </div>
      </div>

      {/* Summary Stats */}
      <div className="mt-8">
        <CapacitySummaryStats providers={providers} />
      </div>

      {/* Filters */}
      <div className="mt-6">
        <CapacityFilters filters={filters} onChange={setFilters} />
        {activeFiltersCount > 0 && (
          <div className="mt-2 flex items-center gap-2">
            <span className="text-xs text-surface-500">
              {activeFiltersCount} filter{activeFiltersCount !== 1 ? "s" : ""} active
            </span>
            <button
              onClick={() => setFilters({ gpuType: "", region: "", status: "" })}
              className="text-xs text-blue-600 hover:text-blue-800 dark:text-blue-400"
            >
              Clear all
            </button>
            <span className="text-xs text-surface-400">
              Showing {filteredProviders.length} of {providers.length} providers
            </span>
          </div>
        )}
      </div>

      {/* Legend */}
      <div className="mt-6">
        <Legend />
      </div>

      {/* Main content: Grid + Detail */}
      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-4">
        <div className="lg:col-span-3">
          <CapacityGrid
            providers={filteredProviders}
            selectedId={selectedId}
            onSelect={setSelectedId}
          />
        </div>
        <div>
          {selectedProvider ? (
            <ProviderDetail
              provider={selectedProvider}
              onClose={() => setSelectedId(null)}
            />
          ) : (
            <div className="rounded-xl border border-surface-200 bg-white p-6 text-center shadow-sm dark:border-surface-800 dark:bg-surface-900">
              <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-full bg-surface-100 text-2xl dark:bg-surface-800">
                &#x1F50D;
              </div>
              <p className="mt-3 text-sm font-medium text-surface-900 dark:text-surface-50">
                Select a Provider
              </p>
              <p className="mt-1 text-xs text-surface-500">
                Click on any provider node to view detailed capacity information
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Regional Distribution */}
      <div className="mt-6">
        <RegionDistribution providers={filteredProviders} />
      </div>

      {/* Footer */}
      <div className="mt-8 rounded-lg bg-surface-50 px-4 py-3 dark:bg-surface-800/50">
        <p className="text-xs text-surface-500">
          Heatmap colors represent GPU utilization levels. Data refreshes every 5s when auto-refresh is enabled.
          All provider data is simulated for demonstration purposes.
        </p>
      </div>
    </div>
  );
}
