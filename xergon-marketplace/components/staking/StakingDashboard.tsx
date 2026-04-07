"use client";

import { useState, useEffect } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

type BondStatus = "active" | "unbonding" | "completed" | "slashed";

interface ActiveBond {
  id: string;
  provider: string;
  amount: number;
  apy: number;
  duration: number;
  remaining: number;
  earned: number;
  status: BondStatus;
  startBlock: number;
}

interface RewardEntry {
  id: string;
  amount: number;
  source: string;
  timestamp: string;
  type: "staking" | "performance" | "referral";
}

interface StakingTier {
  name: string;
  minStake: number;
  apy: string;
  benefits: string[];
  color: string;
  bg: string;
  border: string;
}

// ── Mock Data ──

const MOCK_BONDS: ActiveBond[] = [
  { id: "b1", provider: "NeuralForge", amount: 2000, apy: 12.5, duration: 180, remaining: 142, earned: 47.3, status: "active", startBlock: 842100 },
  { id: "b2", provider: "GPUHive", amount: 1000, apy: 10.2, duration: 90, remaining: 34, earned: 12.8, status: "active", startBlock: 851300 },
  { id: "b3", provider: "InferX", amount: 500, apy: 8.7, duration: 30, remaining: 0, earned: 3.6, status: "completed", startBlock: 849000 },
  { id: "b4", provider: "TensorNode", amount: 750, apy: 11.0, duration: 365, remaining: 290, earned: 22.5, status: "active", startBlock: 838000 },
];

const MOCK_REWARDS: RewardEntry[] = [
  { id: "r1", amount: 5.23, source: "NeuralForge", timestamp: "2 hours ago", type: "staking" },
  { id: "r2", amount: 2.14, source: "Performance bonus", timestamp: "6 hours ago", type: "performance" },
  { id: "r3", amount: 1.50, source: "GPUHive", timestamp: "1 day ago", type: "staking" },
  { id: "r4", amount: 10.00, source: "Referral: 0x3f2a...", timestamp: "2 days ago", type: "referral" },
  { id: "r5", amount: 3.67, source: "TensorNode", timestamp: "3 days ago", type: "staking" },
  { id: "r6", amount: 0.85, source: "Performance bonus", timestamp: "4 days ago", type: "performance" },
  { id: "r7", amount: 3.60, source: "InferX (completed)", timestamp: "5 days ago", type: "staking" },
  { id: "r8", amount: 2.30, source: "NeuralForge", timestamp: "1 week ago", type: "staking" },
];

const MOCK_TIERS: StakingTier[] = [
  {
    name: "Bronze",
    minStake: 100,
    apy: "6-8%",
    benefits: ["Basic staking rewards", "Standard APY", "Community support"],
    color: "text-amber-700",
    bg: "bg-amber-50",
    border: "border-amber-200",
  },
  {
    name: "Silver",
    minStake: 500,
    apy: "8-12%",
    benefits: ["Enhanced APY", "Priority queue access", "Early feature access"],
    color: "text-slate-600",
    bg: "bg-slate-50",
    border: "border-slate-200",
  },
  {
    name: "Gold",
    minStake: 2000,
    apy: "12-18%",
    benefits: ["Premium APY rates", "Reputation boost", "Dedicated support", "Governance voting"],
    color: "text-yellow-700",
    bg: "bg-yellow-50",
    border: "border-yellow-200",
  },
  {
    name: "Diamond",
    minStake: 10000,
    apy: "18-25%",
    benefits: ["Maximum APY", "Exclusive provider access", "Multiplied reputation", "VIP support", "Protocol revenue share"],
    color: "text-violet-700",
    bg: "bg-violet-50",
    border: "border-violet-200",
  },
];

const STATUS_STYLES: Record<BondStatus, string> = {
  active: "bg-emerald-50 text-emerald-600",
  unbonding: "bg-amber-50 text-amber-600",
  completed: "bg-surface-100 text-surface-800/50",
  slashed: "bg-red-50 text-red-500",
};

const DURATION_OPTIONS = [
  { label: "30 Days", value: 30, apyBase: 8 },
  { label: "90 Days", value: 90, apyBase: 10 },
  { label: "180 Days", value: 180, apyBase: 14 },
  { label: "365 Days", value: 365, apyBase: 20 },
];

// ── Component ──

export default function StakingDashboard() {
  const [stakeAmount, setStakeAmount] = useState("");
  const [selectedDuration, setSelectedDuration] = useState(DURATION_OPTIONS[1]);
  const [isLoading, setIsLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<"bonds" | "rewards" | "calculator" | "tiers">("bonds");

  const totalStaked = MOCK_BONDS.filter((b) => b.status === "active").reduce((sum, b) => sum + b.amount, 0);
  const totalEarned = MOCK_REWARDS.reduce((sum, r) => sum + r.amount, 0);
  const currentAPY = 11.2;
  const activeBonds = MOCK_BONDS.filter((b) => b.status === "active").length;

  const projectedReturn = (() => {
    const amt = parseFloat(stakeAmount) || 0;
    return ((amt * selectedDuration.apyBase) / 100) * (selectedDuration.value / 365);
  })();

  useEffect(() => {
    const timer = setTimeout(() => setIsLoading(false), 500);
    return () => clearTimeout(timer);
  }, []);

  const handleStake = () => {
    if (!stakeAmount || parseFloat(stakeAmount) <= 0) return;
    alert(`Would stake ${stakeAmount} ERG for ${selectedDuration.label} at ~${selectedDuration.apyBase}% APY. (Mock action)`);
  };

  // ── Loading ──
  if (isLoading) {
    return (
      <div className="min-h-screen bg-surface-50 p-4 md:p-8">
        <div className="max-w-6xl mx-auto space-y-6">
          <div className="h-8 w-56 rounded-lg bg-surface-100 animate-pulse mb-2" />
          <div className="h-4 w-80 rounded bg-surface-100 animate-pulse" />
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} className="h-28 rounded-xl bg-surface-100 animate-pulse" />
            ))}
          </div>
          <div className="h-72 rounded-xl bg-surface-100 animate-pulse" />
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-surface-50 p-4 md:p-8">
      <div className="max-w-6xl mx-auto space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold text-surface-900 mb-1">Staking Dashboard</h1>
          <p className="text-surface-800/60">Stake ERG to earn rewards, boost reputation, and support the network.</p>
        </div>

        {/* Overview Cards */}
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4">
          {[
            { label: "Total Staked", value: `${totalStaked.toLocaleString()} ERG`, sub: `${activeBonds} active bonds`, icon: "🔒" },
            { label: "Earned Rewards", value: `${totalEarned.toFixed(2)} ERG`, sub: "All time", icon: "💰" },
            { label: "Current APY", value: `${currentAPY}%`, sub: "Weighted average", icon: "📈" },
            { label: "Active Bonds", value: String(activeBonds), sub: "Across 3 providers", icon: "📋" },
          ].map((card) => (
            <div key={card.label} className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <div className="flex items-center justify-between mb-3">
                <span className="text-xs font-medium text-surface-800/50">{card.label}</span>
                <span className="text-lg">{card.icon}</span>
              </div>
              <p className="text-xl font-bold text-surface-900">{card.value}</p>
              <p className="text-xs text-surface-800/40 mt-1">{card.sub}</p>
            </div>
          ))}
        </div>

        {/* Stake Form */}
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="text-lg font-semibold text-surface-900 mb-4">New Stake</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div>
              <label className="block text-sm font-medium text-surface-800/70 mb-2">Amount (ERG)</label>
              <div className="relative">
                <input
                  type="number"
                  value={stakeAmount}
                  onChange={(e) => setStakeAmount(e.target.value)}
                  placeholder="0.00"
                  min="0"
                  className="w-full rounded-xl border border-surface-200 bg-surface-0 px-4 py-3 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20 transition-colors"
                />
                <span className="absolute right-4 top-1/2 -translate-y-1/2 text-sm text-surface-800/40">ERG</span>
              </div>
              <div className="flex gap-2 mt-2">
                {[100, 500, 1000, 5000].map((preset) => (
                  <button
                    key={preset}
                    onClick={() => setStakeAmount(String(preset))}
                    className="px-3 py-1 text-xs rounded-lg bg-surface-50 text-surface-800/60 hover:bg-surface-100 hover:text-surface-900 transition-colors font-medium"
                  >
                    {preset >= 1000 ? `${preset / 1000}k` : preset}
                  </button>
                ))}
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-surface-800/70 mb-2">Lock Duration</label>
              <div className="grid grid-cols-2 gap-2">
                {DURATION_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setSelectedDuration(opt)}
                    className={cn(
                      "rounded-xl border p-3 text-left transition-all",
                      selectedDuration.value === opt.value
                        ? "border-brand-500 bg-brand-50/30"
                        : "border-surface-200 bg-surface-0 hover:border-surface-300"
                    )}
                  >
                    <span className={cn("text-sm font-semibold", selectedDuration.value === opt.value ? "text-brand-600" : "text-surface-900")}>
                      {opt.label}
                    </span>
                    <span className="block text-xs text-surface-800/50 mt-0.5">~{opt.apyBase}% APY</span>
                  </button>
                ))}
              </div>
            </div>
          </div>
          {parseFloat(stakeAmount) > 0 && (
            <div className="mt-4 p-4 rounded-lg bg-surface-50">
              <div className="flex items-center justify-between">
                <span className="text-sm text-surface-800/60">Projected Return</span>
                <span className="text-lg font-bold text-emerald-600">+{projectedReturn.toFixed(2)} ERG</span>
              </div>
              <p className="text-xs text-surface-800/40 mt-1">
                Estimated over {selectedDuration.label} at ~{selectedDuration.apyBase}% APY
              </p>
            </div>
          )}
          <button
            onClick={handleStake}
            disabled={!stakeAmount || parseFloat(stakeAmount) <= 0}
            className={cn(
              "mt-4 w-full rounded-xl font-medium py-3 text-sm transition-colors",
              stakeAmount && parseFloat(stakeAmount) > 0
                ? "bg-brand-500 text-white hover:bg-brand-600"
                : "bg-surface-100 text-surface-800/30 cursor-not-allowed"
            )}
          >
            Bond {stakeAmount || "0"} ERG for {selectedDuration.label}
          </button>
        </div>

        {/* Tabs */}
        <div className="flex flex-wrap gap-1 border-b border-surface-200 pb-px">
          {(["bonds", "rewards", "calculator", "tiers"] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={cn(
                "px-4 py-2.5 text-sm font-medium rounded-t-lg capitalize transition-colors",
                activeTab === tab
                  ? "border-brand-500 text-brand-600 bg-brand-50/30 border-b-2 -mb-px"
                  : "border-transparent text-surface-800/50 hover:text-surface-900 hover:bg-surface-50"
              )}
            >
              {tab}
            </button>
          ))}
        </div>

        {/* ═══ Bonds Tab ═══ */}
        {activeTab === "bonds" && (
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-surface-200 bg-surface-50">
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Provider</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Amount</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">APY</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Duration</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Earned</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Status</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-surface-100">
                  {MOCK_BONDS.map((bond) => (
                    <tr key={bond.id} className="hover:bg-surface-50 transition-colors">
                      <td className="px-6 py-4 text-sm font-medium text-surface-900">{bond.provider}</td>
                      <td className="px-6 py-4 text-sm text-surface-900">{bond.amount.toLocaleString()} ERG</td>
                      <td className="px-6 py-4 text-sm font-semibold text-emerald-600">{bond.apy}%</td>
                      <td className="px-6 py-4 text-sm text-surface-800/70">
                        {bond.duration}d
                        {bond.remaining > 0 && <span className="text-surface-800/40"> ({bond.remaining}d left)</span>}
                      </td>
                      <td className="px-6 py-4 text-sm text-surface-900">{bond.earned.toFixed(2)} ERG</td>
                      <td className="px-6 py-4">
                        <span className={cn("text-xs font-semibold px-2 py-0.5 rounded-full capitalize", STATUS_STYLES[bond.status])}>
                          {bond.status}
                        </span>
                      </td>
                      <td className="px-6 py-4">
                        {bond.status === "active" ? (
                          <button className="text-xs font-medium text-brand-600 hover:text-brand-700 transition-colors">
                            Unbond
                          </button>
                        ) : bond.status === "completed" ? (
                          <button className="text-xs font-medium text-emerald-600 hover:text-emerald-700 transition-colors">
                            Claim
                          </button>
                        ) : (
                          <span className="text-xs text-surface-800/30">—</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {/* ═══ Rewards Tab ═══ */}
        {activeTab === "rewards" && (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
            <h2 className="text-lg font-semibold text-surface-900 mb-4">Rewards History</h2>
            <div className="space-y-3">
              {MOCK_REWARDS.map((reward) => (
                <div key={reward.id} className="flex items-center justify-between p-3 rounded-lg hover:bg-surface-50 transition-colors">
                  <div className="flex items-center gap-3">
                    <div className={cn(
                      "w-8 h-8 rounded-full flex items-center justify-center text-sm",
                      reward.type === "staking" ? "bg-emerald-50" :
                      reward.type === "performance" ? "bg-blue-50" :
                      "bg-purple-50"
                    )}>
                      {reward.type === "staking" ? "💰" : reward.type === "performance" ? "⭐" : "🎁"}
                    </div>
                    <div>
                      <p className="text-sm text-surface-900">{reward.source}</p>
                      <span className="text-xs text-surface-800/40">{reward.timestamp}</span>
                    </div>
                  </div>
                  <span className="text-sm font-semibold text-emerald-600">+{reward.amount.toFixed(2)} ERG</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* ═══ Calculator Tab ═══ */}
        {activeTab === "calculator" && (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
            <h2 className="text-lg font-semibold text-surface-900 mb-4">Yield Calculator</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-surface-800/70 mb-2">Stake Amount (ERG)</label>
                  <input
                    type="number"
                    value={stakeAmount}
                    onChange={(e) => setStakeAmount(e.target.value)}
                    placeholder="1000"
                    min="0"
                    className="w-full rounded-xl border border-surface-200 bg-surface-0 px-4 py-3 text-sm text-surface-900 placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500/20 transition-colors"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-surface-800/70 mb-2">Duration (days)</label>
                  <input
                    type="number"
                    value={selectedDuration.value}
                    readOnly
                    className="w-full rounded-xl border border-surface-200 bg-surface-50 px-4 py-3 text-sm text-surface-900"
                  />
                  <div className="flex gap-2 mt-2">
                    {DURATION_OPTIONS.map((opt) => (
                      <button
                        key={opt.value}
                        onClick={() => setSelectedDuration(opt)}
                        className={cn(
                          "px-3 py-1 text-xs rounded-lg font-medium transition-colors",
                          selectedDuration.value === opt.value
                            ? "bg-brand-500 text-white"
                            : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"
                        )}
                      >
                        {opt.label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="rounded-xl bg-surface-50 p-6 space-y-4">
                <h3 className="text-sm font-semibold text-surface-800/70">Projected Returns</h3>
                {[
                  { label: "Stake Amount", value: `${parseFloat(stakeAmount || "0").toLocaleString()} ERG` },
                  { label: "Duration", value: `${selectedDuration.value} days` },
                  { label: "Estimated APY", value: `~${selectedDuration.apyBase}%` },
                  { label: "Gross Return", value: `${projectedReturn.toFixed(2)} ERG`, highlight: true },
                  { label: "Net Return (after fees)", value: `${(projectedReturn * 0.95).toFixed(2)} ERG`, highlight: true },
                ].map((row) => (
                  <div key={row.label} className="flex items-center justify-between">
                    <span className="text-sm text-surface-800/60">{row.label}</span>
                    <span className={cn("text-sm font-semibold", row.highlight ? "text-emerald-600" : "text-surface-900")}>
                      {row.value}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* ═══ Tiers Tab ═══ */}
        {activeTab === "tiers" && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {MOCK_TIERS.map((tier) => (
              <div key={tier.name} className={cn("rounded-xl border p-6", tier.border, tier.bg)}>
                <div className="flex items-center justify-between mb-3">
                  <h3 className={cn("text-lg font-bold", tier.color)}>{tier.name}</h3>
                  <span className={cn("text-sm font-bold", tier.color)}>{tier.apy} APY</span>
                </div>
                <p className="text-sm text-surface-800/60 mb-4">Minimum: {tier.minStake.toLocaleString()} ERG</p>
                <ul className="space-y-2">
                  {tier.benefits.map((benefit) => (
                    <li key={benefit} className="flex items-center gap-2 text-sm text-surface-900">
                      <span className="text-emerald-500 text-xs">✓</span>
                      {benefit}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
