"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { useAuthStore } from "@/lib/stores/auth";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Tab = "overview" | "providers" | "users" | "models" | "settings";

interface AdminOverview {
  totalProviders: number;
  activeProviders: number;
  totalUsers: number;
  totalRequests: number;
  totalErgStaked: number;
  totalRevenue: number;
  requests24h: number;
  avgLatencyMs: number;
  providerGrowth: number;
}

interface AdminProvider {
  id: string;
  name: string;
  region: string;
  models: string[];
  status: "active" | "suspended";
  uptime: number;
  ergStaked: number;
  revenue: number;
  lastSeen: string;
}

interface AdminUser {
  id: string;
  address: string;
  role: "user" | "admin" | "banned";
  joinedAt: string;
  requestsCount: number;
  spent: number;
  lastActive: string;
}

interface AdminModel {
  id: string;
  name: string;
  enabled: boolean;
  pricingNanoerg: number;
  providersCount: number;
  requests24h: number;
  revenue24h: number;
}

interface AdminSettings {
  feeRateBps: number;
  minStakeErg: number;
  maxRequestsPerMinute: number;
  maxTokensPerRequest: number;
  networkName: string;
  maintenanceMode: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  if (erg >= 1_000) return `${(erg / 1_000).toFixed(1)}K ERG`;
  return `${erg.toFixed(2)} ERG`;
}

function truncateAddr(addr: string, len = 8): string {
  if (addr.length <= len * 2 + 3) return addr;
  return `${addr.slice(0, len)}...${addr.slice(-len)}`;
}

// ---------------------------------------------------------------------------
// Simple SVG icons
// ---------------------------------------------------------------------------

function IconServer() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="2" width="20" height="8" rx="2" ry="2" />
      <rect x="2" y="14" width="20" height="8" rx="2" ry="2" />
      <line x1="6" y1="6" x2="6.01" y2="6" />
      <line x1="6" y1="18" x2="6.01" y2="18" />
    </svg>
  );
}
function IconUsers() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 00-3-3.87" />
      <path d="M16 3.13a4 4 0 010 7.75" />
    </svg>
  );
}
function IconCoins() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <path d="M16 8h-6a2 2 0 00-2 2v1a2 2 0 002 2h4a2 2 0 012 2v1a2 2 0 01-2 2H8" />
      <path d="M12 18V6" />
    </svg>
  );
}
function IconZap() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
    </svg>
  );
}
function IconClock() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  );
}
function IconCube() {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
      <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
      <line x1="12" y1="22.08" x2="12" y2="12" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Simple bar chart
// ---------------------------------------------------------------------------

function MiniBarChart({ data, color = "bg-brand-500" }: { data: number[]; color?: string }) {
  const max = Math.max(...data, 1);
  return (
    <div className="flex items-end gap-0.5 h-16">
      {data.map((v, i) => (
        <div
          key={i}
          className={`flex-1 rounded-t ${color} min-w-[2px] transition-all`}
          style={{ height: `${Math.max((v / max) * 100, 4)}%` }}
        />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Metric card
// ---------------------------------------------------------------------------

function MetricCard({ label, value, icon, trend }: { label: string; value: string; icon: React.ReactNode; trend?: string }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 transition-all hover:shadow-md">
      <div className="flex items-start justify-between mb-2">
        <div className="rounded-lg bg-brand-50 p-2 text-brand-600 dark:bg-brand-950/30">{icon}</div>
        {trend && (
          <span className={`text-xs font-medium ${trend.startsWith("+") ? "text-emerald-600 dark:text-emerald-400" : "text-red-500"}`}>
            {trend}
          </span>
        )}
      </div>
      <div className="text-xl font-bold text-surface-900 mb-0.5">{value}</div>
      <div className="text-xs text-surface-800/50">{label}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function LoadingSkeleton() {
  return (
    <div className="space-y-6 animate-pulse">
      <div className="h-8 w-48 rounded-lg bg-surface-200" />
      <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4">
        {Array.from({ length: 6 }).map((_, i) => (
          <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <div className="h-8 w-8 rounded-lg bg-surface-200 mb-3" />
            <div className="h-7 w-24 bg-surface-200 rounded mb-1.5" />
            <div className="h-4 w-16 bg-surface-100 rounded" />
          </div>
        ))}
      </div>
      <div className="h-48 rounded-xl border border-surface-200 bg-surface-0" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tab definitions
// ---------------------------------------------------------------------------

const TABS: Array<{ id: Tab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "providers", label: "Providers" },
  { id: "users", label: "Users" },
  { id: "models", label: "Models" },
  { id: "settings", label: "Settings" },
];

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function AdminDashboardPage() {
  const user = useAuthStore((s) => s.user);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoading = useAuthStore((s) => s.isLoading);
  const [tab, setTab] = useState<Tab>("overview");
  const [sidebarOpen, setSidebarOpen] = useState(false);

  const [overview, setOverview] = useState<AdminOverview | null>(null);
  const [providers, setProviders] = useState<AdminProvider[]>([]);
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [settings, setSettings] = useState<AdminSettings | null>(null);
  const [loading, setLoading] = useState(true);

  // Mock chart data
  const requestsChartData = useMemo(() => Array.from({ length: 14 }, () => Math.floor(Math.random() * 5000) + 1000), []);
  const revenueChartData = useMemo(() => Array.from({ length: 14 }, () => Math.floor(Math.random() * 2000) + 500), []);
  const growthChartData = useMemo(() => Array.from({ length: 14 }, (_, i) => 20 + i * 3 + Math.floor(Math.random() * 10)), []);

  // Load all admin data
  const loadAllData = useCallback(async () => {
    try {
      const res = await fetch("/api/admin/dashboard").catch(() => null);
      if (res?.ok) {
        const data = await res.json();
        setOverview(data.overview);
        setProviders(data.providers || []);
        setUsers(data.users || []);
        setModels(data.models || []);
        setSettings(data.settings);
      } else {
        // Mock data
        setOverview({
          totalProviders: 24,
          activeProviders: 18,
          totalUsers: 1_250,
          totalRequests: 3_500_000,
          totalErgStaked: 12_500_000_000_000,
          totalRevenue: 450_000_000_000,
          requests24h: 45_000,
          avgLatencyMs: 180,
          providerGrowth: 12,
        });
        setProviders(generateMockProviders());
        setUsers(generateMockUsers());
        setModels(generateMockModels());
        setSettings({
          feeRateBps: 250,
          minStakeErg: 10,
          maxRequestsPerMinute: 60,
          maxTokensPerRequest: 8192,
          networkName: "Xergon Testnet",
          maintenanceMode: false,
        });
      }
    } catch {
      setOverview({
        totalProviders: 24, activeProviders: 18, totalUsers: 1_250, totalRequests: 3_500_000,
        totalErgStaked: 12_500_000_000_000, totalRevenue: 450_000_000_000,
        requests24h: 45_000, avgLatencyMs: 180, providerGrowth: 12,
      });
      setProviders(generateMockProviders());
      setUsers(generateMockUsers());
      setModels(generateMockModels());
      setSettings({
        feeRateBps: 250, minStakeErg: 10, maxRequestsPerMinute: 60,
        maxTokensPerRequest: 8192, networkName: "Xergon Testnet", maintenanceMode: false,
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadAllData();
  }, [loadAllData]);

  // Auth guard
  if (isLoading) return <LoadingSkeleton />;

  if (!isAuthenticated || !user) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-16 text-center">
        <svg className="w-16 h-16 mx-auto mb-4 text-surface-300" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
        <h2 className="text-xl font-bold text-surface-900 mb-2">Access Denied</h2>
        <p className="text-sm text-surface-800/50">You must be authenticated to access the admin dashboard.</p>
      </div>
    );
  }

  return (
    <div className="flex min-h-[calc(100dvh-4rem)]">
      {/* Sidebar - desktop */}
      <aside className="hidden lg:flex flex-col w-56 border-r border-surface-200 bg-surface-0 p-4 flex-shrink-0">
        <div className="mb-6">
          <h2 className="text-sm font-bold text-surface-900">Admin Panel</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">{truncateAddr(user.publicKey)}</p>
        </div>
        <nav className="flex flex-col gap-1">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              className={`px-3 py-2 rounded-lg text-sm font-medium text-left transition-colors ${
                tab === t.id
                  ? "bg-brand-50 text-brand-700 dark:bg-brand-950/30 dark:text-brand-400"
                  : "text-surface-800/60 hover:bg-surface-50 hover:text-surface-800/80"
              }`}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </aside>

      {/* Mobile sidebar toggle + tab bar */}
      <div className="lg:hidden fixed bottom-0 left-0 right-0 z-50 border-t border-surface-200 bg-surface-0 flex overflow-x-auto">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTab(t.id)}
            className={`flex-1 min-w-0 px-2 py-3 text-xs font-medium text-center transition-colors whitespace-nowrap ${
              tab === t.id
                ? "text-brand-600 border-t-2 border-brand-600"
                : "text-surface-800/50"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Main content */}
      <main className="flex-1 min-w-0 p-4 md:p-6 pb-20 lg:pb-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold text-surface-900">Admin Dashboard</h1>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Network overview, provider management, and system configuration
            </p>
          </div>
          <button
            type="button"
            onClick={loadAllData}
            className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-surface-200 bg-surface-0 text-sm font-medium text-surface-800/70 hover:bg-surface-50 transition-colors"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="23 4 23 10 17 10" />
              <path d="M20.49 15a9 9 0 11-2.12-9.36L23 10" />
            </svg>
            Refresh
          </button>
        </div>

        {loading ? (
          <LoadingSkeleton />
        ) : (
          <>
            {tab === "overview" && renderOverview(overview!, requestsChartData, revenueChartData, growthChartData)}
            {tab === "providers" && renderProviders(providers)}
            {tab === "users" && renderUsers(users)}
            {tab === "models" && renderModels(models)}
            {tab === "settings" && renderSettings(settings!)}
          </>
        )}
      </main>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tab renderers
// ---------------------------------------------------------------------------

function renderOverview(ov: AdminOverview, reqData: number[], revData: number[], growthData: number[]) {
  const metrics = [
    { label: "Total Providers", value: String(ov.totalProviders), icon: <IconServer />, trend: `+${ov.providerGrowth}%` },
    { label: "Active Providers", value: String(ov.activeProviders), icon: <IconServer /> },
    { label: "Total Users", value: formatNumber(ov.totalUsers), icon: <IconUsers />, trend: "+8.2%" },
    { label: "ERG Staked", value: nanoergToErg(ov.totalErgStaked), icon: <IconCoins /> },
    { label: "Revenue", value: nanoergToErg(ov.totalRevenue), icon: <IconCoins />, trend: "+15.3%" },
    { label: "Requests (24h)", value: formatNumber(ov.requests24h), icon: <IconZap />, trend: "+5.1%" },
  ];

  return (
    <div className="space-y-6">
      {/* Metric cards */}
      <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4">
        {metrics.map((m) => (
          <MetricCard key={m.label} {...m} />
        ))}
      </div>

      {/* Charts row */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Requests (14d)</h3>
          <MiniBarChart data={reqData} color="bg-brand-500" />
          <div className="flex justify-between mt-2 text-[10px] text-surface-800/30">
            <span>14 days ago</span>
            <span>Today</span>
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Revenue (14d)</h3>
          <MiniBarChart data={revData} color="bg-emerald-500" />
          <div className="flex justify-between mt-2 text-[10px] text-surface-800/30">
            <span>14 days ago</span>
            <span>Today</span>
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Provider Growth</h3>
          <MiniBarChart data={growthData} color="bg-violet-500" />
          <div className="flex justify-between mt-2 text-[10px] text-surface-800/30">
            <span>14 days ago</span>
            <span>Today</span>
          </div>
        </div>
      </div>

      {/* Quick stats */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Performance</h3>
          <div className="space-y-2">
            <QuickStat label="Avg Latency" value={`${ov.avgLatencyMs}ms`} />
            <QuickStat label="Uptime" value="99.7%" />
            <QuickStat label="Error Rate" value="0.3%" />
            <QuickStat label="P95 Latency" value="320ms" />
          </div>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Network Health</h3>
          <div className="space-y-2">
            <QuickStat label="Relay Status" value="Healthy" highlight />
            <QuickStat label="Node Height" value="1,245,890" />
            <QuickStat label="Active Models" value="12" />
            <QuickStat label="Staking Boxes" value="24" />
          </div>
        </div>
      </div>
    </div>
  );
}

function renderProviders(providers: AdminProvider[]) {
  const handleToggleStatus = (id: string) => {
    setProvidersData(id); // would call API
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-surface-900">Provider Management</h2>
        <span className="text-sm text-surface-800/40">{providers.length} providers</span>
      </div>

      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-100 bg-surface-50/50">
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Provider</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Region</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Status</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Uptime</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Staked</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Revenue</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Actions</th>
              </tr>
            </thead>
            <tbody>
              {providers.map((p) => (
                <tr key={p.id} className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors">
                  <td className="py-3 px-4">
                    <div className="font-medium text-surface-900">{p.name}</div>
                    <div className="text-xs text-surface-800/40">{p.models.length} models</div>
                  </td>
                  <td className="py-3 px-4 text-surface-800/70">{p.region}</td>
                  <td className="py-3 px-4">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${
                      p.status === "active" ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-400"
                        : "bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-400"
                    }`}>
                      {p.status}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-surface-800/70">{p.uptime.toFixed(1)}%</td>
                  <td className="py-3 px-4">{nanoergToErg(p.ergStaked)}</td>
                  <td className="py-3 px-4">{nanoergToErg(p.revenue)}</td>
                  <td className="py-3 px-4">
                    <div className="flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => handleToggleStatus(p.id)}
                        className={`px-2 py-1 text-xs rounded font-medium transition-colors ${
                          p.status === "active"
                            ? "bg-red-50 text-red-600 hover:bg-red-100"
                            : "bg-emerald-50 text-emerald-600 hover:bg-emerald-100"
                        }`}
                      >
                        {p.status === "active" ? "Suspend" : "Activate"}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function renderUsers(users: AdminUser[]) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-surface-900">User Management</h2>
        <span className="text-sm text-surface-800/40">{users.length} users</span>
      </div>

      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-100 bg-surface-50/50">
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Address</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Role</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Requests</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Spent</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Last Active</th>
                <th className="text-left py-3 px-4 text-xs font-semibold text-surface-800/50 uppercase tracking-wide">Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id} className="border-b border-surface-50 last:border-0 hover:bg-surface-50/50 transition-colors">
                  <td className="py-3 px-4 font-mono text-xs text-surface-800/70">{truncateAddr(u.address, 12)}</td>
                  <td className="py-3 px-4">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${
                      u.role === "admin" ? "bg-brand-50 text-brand-700 dark:bg-brand-950/30 dark:text-brand-400"
                        : u.role === "banned" ? "bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-400"
                        : "bg-surface-100 text-surface-700 dark:bg-surface-800 dark:text-surface-300"
                    }`}>
                      {u.role}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-surface-800/70">{formatNumber(u.requestsCount)}</td>
                  <td className="py-3 px-4">{nanoergToErg(u.spent)}</td>
                  <td className="py-3 px-4 text-xs text-surface-800/40">
                    {new Date(u.lastActive).toLocaleDateString()}
                  </td>
                  <td className="py-3 px-4">
                    <div className="flex items-center gap-1">
                      {u.role !== "banned" ? (
                        <button
                          type="button"
                          className="px-2 py-1 text-xs rounded font-medium bg-red-50 text-red-600 hover:bg-red-100 transition-colors"
                        >
                          Ban
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="px-2 py-1 text-xs rounded font-medium bg-emerald-50 text-emerald-600 hover:bg-emerald-100 transition-colors"
                        >
                          Unban
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function renderModels(models: AdminModel[]) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-surface-900">Model Management</h2>
        <span className="text-sm text-surface-800/40">{models.length} models</span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {models.map((m) => (
          <div key={m.id} className="rounded-xl border border-surface-200 bg-surface-0 p-4">
            <div className="flex items-start justify-between mb-3">
              <div>
                <h3 className="font-medium text-surface-900">{m.name}</h3>
                <div className="text-xs text-surface-800/40 mt-0.5">
                  {m.providersCount} providers | {formatNumber(m.requests24h)} req/24h
                </div>
              </div>
              <button
                type="button"
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${m.enabled ? "bg-emerald-500" : "bg-surface-300"}`}
              >
                <span className={`inline-block h-4 w-4 rounded-full bg-white transition-transform ${m.enabled ? "translate-x-6" : "translate-x-1"}`} />
              </button>
            </div>
            <div className="flex items-center justify-between text-xs">
              <span className="text-surface-800/50">Pricing</span>
              <span className="font-medium text-surface-900">{nanoergToErg(m.pricingNanoerg)}/1M tokens</span>
            </div>
            <div className="flex items-center justify-between text-xs mt-1">
              <span className="text-surface-800/50">Revenue (24h)</span>
              <span className="font-medium text-surface-900">{nanoergToErg(m.revenue24h)}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function renderSettings(s: AdminSettings) {
  const [localSettings, setLocalSettings] = useState(s);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    setSaving(true);
    await new Promise((r) => setTimeout(r, 800));
    setSaving(false);
  };

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold text-surface-900">System Configuration</h2>

      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 space-y-5">
        {/* Network config */}
        <SettingsSection title="Network Configuration">
          <SettingsRow label="Network Name" description="Display name for this network instance">
            <input
              type="text"
              value={localSettings.networkName}
              onChange={(e) => setLocalSettings((prev) => ({ ...prev, networkName: e.target.value }))}
              className="w-48 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </SettingsRow>
          <SettingsRow label="Maintenance Mode" description="Disable all user-facing features">
            <button
              type="button"
              onClick={() => setLocalSettings((prev) => ({ ...prev, maintenanceMode: !prev.maintenanceMode }))}
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${localSettings.maintenanceMode ? "bg-red-500" : "bg-emerald-500"}`}
            >
              <span className={`inline-block h-4 w-4 rounded-full bg-white transition-transform ${localSettings.maintenanceMode ? "translate-x-6" : "translate-x-1"}`} />
            </button>
          </SettingsRow>
        </SettingsSection>

        {/* Fee rates */}
        <SettingsSection title="Fee Configuration">
          <SettingsRow label="Platform Fee" description="Fee charged on each transaction (basis points)">
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={String(localSettings.feeRateBps)}
                onChange={(e) => setLocalSettings((prev) => ({ ...prev, feeRateBps: Number(e.target.value) || 0 }))}
                className="w-20 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              />
              <span className="text-xs text-surface-800/40">bps ({(localSettings.feeRateBps / 100).toFixed(2)}%)</span>
            </div>
          </SettingsRow>
          <SettingsRow label="Min Stake" description="Minimum ERG required for provider staking">
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={String(localSettings.minStakeErg)}
                onChange={(e) => setLocalSettings((prev) => ({ ...prev, minStakeErg: Number(e.target.value) || 0 }))}
                className="w-20 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
              />
              <span className="text-xs text-surface-800/40">ERG</span>
            </div>
          </SettingsRow>
        </SettingsSection>

        {/* Rate limits */}
        <SettingsSection title="Rate Limits">
          <SettingsRow label="Requests/min" description="Max API requests per user per minute">
            <input
              type="text"
              value={String(localSettings.maxRequestsPerMinute)}
              onChange={(e) => setLocalSettings((prev) => ({ ...prev, maxRequestsPerMinute: Number(e.target.value) || 0 }))}
              className="w-20 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </SettingsRow>
          <SettingsRow label="Max Tokens/Request" description="Maximum output tokens per inference request">
            <input
              type="text"
              value={String(localSettings.maxTokensPerRequest)}
              onChange={(e) => setLocalSettings((prev) => ({ ...prev, maxTokensPerRequest: Number(e.target.value) || 0 }))}
              className="w-20 px-3 py-1.5 text-sm rounded-lg border border-surface-200 bg-surface-0 text-right text-surface-900 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            />
          </SettingsRow>
        </SettingsSection>

        {/* Save button */}
        <div className="pt-3 border-t border-surface-100">
          <button
            type="button"
            onClick={handleSave}
            disabled={saving}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 disabled:opacity-50 transition-colors"
          >
            {saving ? (
              <>
                <svg className="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" className="opacity-25" />
                  <path d="M4 12a8 8 0 018-8" stroke="currentColor" strokeWidth="4" strokeLinecap="round" className="opacity-75" />
                </svg>
                Saving...
              </>
            ) : "Save Configuration"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function QuickStat({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-surface-800/50">{label}</span>
      <span className={`text-sm font-medium ${highlight ? "text-emerald-600" : "text-surface-900"}`}>{value}</span>
    </div>
  );
}

function SettingsSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="text-sm font-semibold text-surface-900 mb-3">{title}</h3>
      <div className="space-y-4">{children}</div>
    </div>
  );
}

function SettingsRow({ label, description, children }: { label: string; description: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <div className="text-sm font-medium text-surface-900">{label}</div>
        <div className="text-xs text-surface-800/40">{description}</div>
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Global setter helper (used by provider actions)
// ---------------------------------------------------------------------------

let _providersRef: React.Dispatch<React.SetStateAction<AdminProvider[]>> | null = null;

function setProvidersData(id: string) {
  if (_providersRef) {
    _providersRef((prev) =>
      prev.map((p) => (p.id === id ? { ...p, status: p.status === "active" ? "suspended" as const : "active" as const } : p)),
    );
  }
}

// ---------------------------------------------------------------------------
// Mock data generators
// ---------------------------------------------------------------------------

function generateMockProviders(): AdminProvider[] {
  const names = ["AlphaNode", "BetaCompute", "GammaAI", "DeltaGPU", "EpsilonML", "ZetaNet", "EtaCloud", "ThetaInfer", "IotaServe", "KappaRun"];
  const regions = ["US", "EU", "Asia"];
  return names.map((name, i) => ({
    id: `prov-${i}`,
    name,
    region: regions[i % regions.length],
    models: [["llama-3.1-70b", "mistral-7b"], ["deepseek-v3", "qwen-2.5-72b"], ["llama-3.1-8b"]][i % 3],
    status: i < 8 ? ("active" as const) : ("suspended" as const),
    uptime: 95 + Math.random() * 5,
    ergStaked: Math.floor(Math.random() * 5_000_000_000_000) + 500_000_000_000,
    revenue: Math.floor(Math.random() * 100_000_000_000) + 1_000_000_000,
    lastSeen: new Date(Date.now() - Math.random() * 3600_000).toISOString(),
  }));
}

function generateMockUsers(): AdminUser[] {
  const roles: Array<"user" | "admin" | "banned"> = ["user", "user", "user", "admin", "user", "banned", "user"];
  return Array.from({ length: 7 }, (_, i) => ({
    id: `user-${i}`,
    address: `${9 + Math.floor(Math.random() * 8)}${Array.from({ length: 30 }, () => "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[Math.floor(Math.random() * 62)]).join("")}`,
    role: roles[i],
    joinedAt: new Date(Date.now() - Math.random() * 30 * 86400_000).toISOString(),
    requestsCount: Math.floor(Math.random() * 10000) + 10,
    spent: Math.floor(Math.random() * 10_000_000_000) + 100_000_000,
    lastActive: new Date(Date.now() - Math.random() * 7 * 86400_000).toISOString(),
  }));
}

function generateMockModels(): AdminModel[] {
  const modelNames = ["llama-3.1-8b", "llama-3.1-70b", "mistral-7b", "deepseek-v3", "qwen-2.5-7b", "qwen-2.5-72b"];
  return modelNames.map((name, i) => ({
    id: `model-${i}`,
    name,
    enabled: i < 5,
    pricingNanoerg: (Math.floor(Math.random() * 50) + 5) * 1_000_000,
    providersCount: Math.floor(Math.random() * 8) + 1,
    requests24h: Math.floor(Math.random() * 5000) + 100,
    revenue24h: Math.floor(Math.random() * 50_000_000_000) + 1_000_000_000,
  }));
}
