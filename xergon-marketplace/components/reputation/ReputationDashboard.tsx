"use client";

import { useState, useEffect } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

type Level = "Bronze" | "Silver" | "Gold" | "Platinum" | "Diamond";

interface ScoreBreakdown {
  category: string;
  key: string;
  value: number;
  icon: string;
}

interface TimelineEvent {
  id: string;
  type: "task_completed" | "stake_boost" | "review" | "milestone" | "penalty";
  description: string;
  timestamp: string;
  impact: number;
}

interface LeaderboardEntry {
  rank: number;
  name: string;
  score: number;
  level: Level;
  change: number;
}

// ── Constants ──

const LEVEL_THRESHOLDS: Record<Level, { min: number; color: string; bg: string; border: string }> = {
  Bronze: { min: 0, color: "text-amber-700", bg: "bg-amber-100", border: "border-amber-300" },
  Silver: { min: 40, color: "text-slate-500", bg: "bg-slate-100", border: "border-slate-300" },
  Gold: { min: 60, color: "text-yellow-600", bg: "bg-yellow-100", border: "border-yellow-400" },
  Platinum: { min: 80, color: "text-cyan-600", bg: "bg-cyan-100", border: "border-cyan-400" },
  Diamond: { min: 95, color: "text-violet-600", bg: "bg-violet-100", border: "border-violet-400" },
};

function getLevel(score: number): Level {
  if (score >= 95) return "Diamond";
  if (score >= 80) return "Platinum";
  if (score >= 60) return "Gold";
  if (score >= 40) return "Silver";
  return "Bronze";
}

const BREAKDOWN_COLORS: Record<string, string> = {
  success_rate: "bg-emerald-500",
  latency: "bg-blue-500",
  uptime: "bg-cyan-500",
  quality: "bg-amber-500",
  community: "bg-purple-500",
};

const TIMELINE_ICONS: Record<string, string> = {
  task_completed: "✓",
  stake_boost: "⚡",
  review: "★",
  milestone: "🏆",
  penalty: "⚠",
};

// ── Mock Data ──

const MOCK_BREAKDOWN: ScoreBreakdown[] = [
  { category: "Success Rate", key: "success_rate", value: 94, icon: "🎯" },
  { category: "Latency", key: "latency", value: 87, icon: "⚡" },
  { category: "Uptime", key: "uptime", value: 99, icon: "🟢" },
  { category: "Quality Score", key: "quality", value: 91, icon: "💎" },
  { category: "Community", key: "community", value: 78, icon: "🤝" },
];

const MOCK_TIMELINE: TimelineEvent[] = [
  { id: "1", type: "task_completed", description: "Completed batch inference job #4821", timestamp: "2 hours ago", impact: +2 },
  { id: "2", type: "review", description: "Received 5-star review from user 0x7f3a...", timestamp: "5 hours ago", impact: +1 },
  { id: "3", type: "stake_boost", description: "Staking boost activated: 500 ERG bonded", timestamp: "1 day ago", impact: +5 },
  { id: "4", type: "task_completed", description: "Completed 100th task this month", timestamp: "2 days ago", impact: +3 },
  { id: "5", type: "milestone", description: "Reached Gold tier reputation", timestamp: "3 days ago", impact: 0 },
  { id: "6", type: "task_completed", description: "Completed image generation job #4756", timestamp: "4 days ago", impact: +1 },
  { id: "7", type: "penalty", description: "Minor timeout on job #4742", timestamp: "5 days ago", impact: -2 },
  { id: "8", type: "review", description: "Received 4-star review from user 0x2b1c...", timestamp: "1 week ago", impact: +1 },
];

const MOCK_LEADERBOARD: LeaderboardEntry[] = [
  { rank: 1, name: "NeuralForge", score: 98, level: "Diamond", change: 0 },
  { rank: 2, name: "GPUHive", score: 97, level: "Diamond", change: +1 },
  { rank: 3, name: "DeepCompute", score: 96, level: "Diamond", change: -1 },
  { rank: 4, name: "InferX", score: 94, level: "Platinum", change: +2 },
  { rank: 5, name: "TensorNode", score: 93, level: "Platinum", change: 0 },
  { rank: 6, name: "ModelMesh", score: 91, level: "Platinum", change: +1 },
  { rank: 7, name: "Your Node", score: 90, level: "Platinum", change: +3 },
  { rank: 8, name: "ComputeEdge", score: 89, level: "Platinum", change: -1 },
  { rank: 9, name: "AICore", score: 87, level: "Gold", change: +2 },
  { rank: 10, name: "PromptGrid", score: 85, level: "Gold", change: 0 },
  { rank: 11, name: "ZeroLatency", score: 83, level: "Gold", change: -2 },
  { rank: 12, name: "SmartBatch", score: 80, level: "Platinum", change: +1 },
  { rank: 13, name: "DataFlow", score: 78, level: "Gold", change: +1 },
  { rank: 14, name: "RunPodLite", score: 72, level: "Gold", change: 0 },
  { rank: 15, name: "FlashGPU", score: 68, level: "Silver", change: -3 },
];

const MOCK_HISTORY = [72, 75, 74, 78, 80, 82, 85, 84, 87, 86, 88, 90];
const HISTORY_LABELS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// ── Component ──

export default function ReputationDashboard() {
  const [activeTab, setActiveTab] = useState<"overview" | "breakdown" | "leaderboard">("overview");
  const [totalScore, setTotalScore] = useState(90);
  const [bondedERG, setBondedERG] = useState(500);
  const [boostAmount, setBoostAmount] = useState(5);
  const [isLoading, setIsLoading] = useState(true);

  const level = getLevel(totalScore);
  const levelStyle = LEVEL_THRESHOLDS[level];
  const nextLevel = Object.entries(LEVEL_THRESHOLDS).find(([, v]) => v.min > totalScore);

  useEffect(() => {
    const timer = setTimeout(() => setIsLoading(false), 600);
    return () => clearTimeout(timer);
  }, []);

  const handleBond = () => {
    setBondedERG((prev) => prev + 100);
    setBoostAmount((prev) => prev + 1);
  };

  // ── Loading Skeleton ──
  if (isLoading) {
    return (
      <div className="min-h-screen bg-surface-50 p-4 md:p-8">
        <div className="max-w-6xl mx-auto space-y-6">
          <div className="h-8 w-64 rounded-lg bg-surface-100 animate-pulse mb-2" />
          <div className="h-4 w-96 rounded bg-surface-100 animate-pulse" />
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {[1, 2, 3].map((i) => (
              <div key={i} className="h-32 rounded-xl bg-surface-100 animate-pulse" />
            ))}
          </div>
          <div className="h-64 rounded-xl bg-surface-100 animate-pulse" />
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-surface-50 p-4 md:p-8">
      <div className="max-w-6xl mx-auto space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold text-surface-900 mb-1">Reputation Dashboard</h1>
          <p className="text-surface-800/60">Track your provider reputation, scores, and leaderboard standing.</p>
        </div>

        {/* Tabs */}
        <div className="flex flex-wrap gap-1 border-b border-surface-200 pb-px">
          {(["overview", "breakdown", "leaderboard"] as const).map((tab) => (
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

        {/* ═══ Overview Tab ═══ */}
        {activeTab === "overview" && (
          <div className="space-y-6">
            {/* Score + Level Cards */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {/* Main Score */}
              <div className="md:col-span-1 rounded-xl border border-surface-200 bg-surface-0 p-6 flex flex-col items-center justify-center">
                <span className="text-sm font-medium text-surface-800/50 mb-2">Reputation Score</span>
                <span className="text-5xl font-bold text-surface-900 mb-3">{totalScore}</span>
                <span className={cn("inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-semibold border", levelStyle.bg, levelStyle.color, levelStyle.border)}>
                  {level === "Diamond" ? "💎" : level === "Platinum" ? "🔵" : level === "Gold" ? "🥇" : level === "Silver" ? "🥈" : "🥉"} {level}
                </span>
                {nextLevel && (
                  <p className="text-xs text-surface-800/40 mt-3">
                    {nextLevel[1].min - totalScore} points to {nextLevel[0]}
                  </p>
                )}
              </div>

              {/* Quick Stats */}
              <div className="md:col-span-2 grid grid-cols-2 gap-3">
                {MOCK_BREAKDOWN.map((item) => (
                  <div key={item.key} className="rounded-xl border border-surface-200 bg-surface-0 p-4">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-medium text-surface-800/50">{item.icon} {item.category}</span>
                      <span className="text-sm font-bold text-surface-900">{item.value}%</span>
                    </div>
                    <div className="h-2 rounded-full bg-surface-100 overflow-hidden">
                      <div
                        className={cn("h-full rounded-full transition-all duration-700", BREAKDOWN_COLORS[item.key])}
                        style={{ width: `${item.value}%` }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Historical Chart */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <h2 className="text-lg font-semibold text-surface-900 mb-4">Score History</h2>
              <div className="flex items-end gap-2 h-40">
                {MOCK_HISTORY.map((val, i) => {
                  const height = ((val - 60) / 40) * 100;
                  return (
                    <div key={i} className="flex-1 flex flex-col items-center gap-1">
                      <span className="text-xs font-medium text-surface-800/60">{val}</span>
                      <div
                        className={cn(
                          "w-full rounded-t-md transition-all duration-500",
                          val >= 95 ? "bg-violet-500" : val >= 80 ? "bg-cyan-500" : val >= 60 ? "bg-yellow-500" : "bg-amber-500"
                        )}
                        style={{ height: `${Math.max(height, 5)}%` }}
                      />
                      <span className="text-[10px] text-surface-800/40">{HISTORY_LABELS[i]}</span>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Activity Timeline */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <h2 className="text-lg font-semibold text-surface-900 mb-4">Recent Activity</h2>
              <div className="space-y-3">
                {MOCK_TIMELINE.map((event) => (
                  <div key={event.id} className="flex items-start gap-3 p-3 rounded-lg hover:bg-surface-50 transition-colors">
                    <div className="w-8 h-8 rounded-full bg-surface-100 flex items-center justify-center text-sm flex-shrink-0">
                      {TIMELINE_ICONS[event.type]}
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm text-surface-900 truncate">{event.description}</p>
                      <span className="text-xs text-surface-800/40">{event.timestamp}</span>
                    </div>
                    {event.impact !== 0 && (
                      <span className={cn(
                        "text-xs font-semibold px-2 py-0.5 rounded-full flex-shrink-0",
                        event.impact > 0 ? "bg-emerald-50 text-emerald-600" : "bg-red-50 text-red-500"
                      )}>
                        {event.impact > 0 ? `+${event.impact}` : event.impact}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </div>

            {/* Staking Boost Section */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold text-surface-900">Staking Boost</h2>
                <span className="text-xs font-medium text-brand-600 bg-brand-50 rounded px-2 py-0.5">Active</span>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div className="rounded-lg bg-surface-50 p-4">
                  <span className="text-xs text-surface-800/50">Bonded ERG</span>
                  <p className="text-xl font-bold text-surface-900 mt-1">{bondedERG.toLocaleString()} ERG</p>
                </div>
                <div className="rounded-lg bg-surface-50 p-4">
                  <span className="text-xs text-surface-800/50">Reputation Boost</span>
                  <p className="text-xl font-bold text-emerald-600 mt-1">+{boostAmount} points</p>
                </div>
                <div className="flex items-end">
                  <button
                    onClick={handleBond}
                    className="w-full rounded-lg bg-brand-500 text-white font-medium py-3 text-sm hover:bg-brand-600 transition-colors"
                  >
                    Bond More ERG
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* ═══ Breakdown Tab ═══ */}
        {activeTab === "breakdown" && (
          <div className="space-y-4">
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <h2 className="text-lg font-semibold text-surface-900 mb-6">Score Breakdown</h2>
              <div className="space-y-5">
                {MOCK_BREAKDOWN.map((item) => (
                  <div key={item.key}>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium text-surface-900">{item.icon} {item.category}</span>
                      <span className="text-sm font-bold text-surface-900">{item.value}/100</span>
                    </div>
                    <div className="h-3 rounded-full bg-surface-100 overflow-hidden">
                      <div
                        className={cn("h-full rounded-full transition-all duration-1000", BREAKDOWN_COLORS[item.key])}
                        style={{ width: `${item.value}%` }}
                      />
                    </div>
                    <p className="text-xs text-surface-800/40 mt-1">
                      {item.value >= 90 ? "Excellent — top tier performance" : item.value >= 80 ? "Good — above average" : item.value >= 70 ? "Fair — room for improvement" : "Needs attention"}
                    </p>
                  </div>
                ))}
              </div>
            </div>

            {/* Tips */}
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
              <h2 className="text-lg font-semibold text-surface-900 mb-4">Improvement Tips</h2>
              <div className="space-y-3">
                {[
                  "Increase community engagement to boost your community score from 78 to 85+.",
                  "Maintain your 99% uptime streak — you're in the top 5% of providers.",
                  "Focus on reducing latency to push your score from 87 toward 95.",
                  "Bond more ERG to unlock higher reputation boost multipliers.",
                ].map((tip, i) => (
                  <div key={i} className="flex items-start gap-3 p-3 rounded-lg bg-brand-50/30">
                    <span className="text-brand-500 text-sm mt-0.5">💡</span>
                    <p className="text-sm text-surface-900">{tip}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* ═══ Leaderboard Tab ═══ */}
        {activeTab === "leaderboard" && (
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <div className="p-6 border-b border-surface-200">
              <h2 className="text-lg font-semibold text-surface-900">Provider Leaderboard</h2>
              <p className="text-sm text-surface-800/50 mt-1">Top 15 providers ranked by reputation score</p>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-surface-200 bg-surface-50">
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Rank</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Provider</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Level</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Score</th>
                    <th className="text-left text-xs font-semibold text-surface-800/50 px-6 py-3 uppercase tracking-wider">Change</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-surface-100">
                  {MOCK_LEADERBOARD.map((entry) => {
                    const lvl = LEVEL_THRESHOLDS[entry.level];
                    const isYou = entry.name === "Your Node";
                    return (
                      <tr key={entry.rank} className={cn("hover:bg-surface-50 transition-colors", isYou && "bg-brand-50/20")}>
                        <td className="px-6 py-4">
                          <span className={cn(
                            "text-sm font-bold w-7 h-7 rounded-full flex items-center justify-center",
                            entry.rank === 1 ? "bg-yellow-100 text-yellow-700" :
                            entry.rank === 2 ? "bg-slate-100 text-slate-600" :
                            entry.rank === 3 ? "bg-amber-100 text-amber-700" :
                            "text-surface-800/60"
                          )}>
                            {entry.rank}
                          </span>
                        </td>
                        <td className="px-6 py-4">
                          <span className={cn("text-sm font-medium", isYou ? "text-brand-600" : "text-surface-900")}>
                            {entry.name}
                            {isYou && <span className="ml-2 text-xs text-brand-500">(you)</span>}
                          </span>
                        </td>
                        <td className="px-6 py-4">
                          <span className={cn("text-xs font-semibold px-2 py-0.5 rounded-full border", lvl.bg, lvl.color, lvl.border)}>
                            {entry.level}
                          </span>
                        </td>
                        <td className="px-6 py-4">
                          <span className="text-sm font-bold text-surface-900">{entry.score}</span>
                        </td>
                        <td className="px-6 py-4">
                          {entry.change > 0 ? (
                            <span className="text-xs font-semibold text-emerald-600">▲ +{entry.change}</span>
                          ) : entry.change < 0 ? (
                            <span className="text-xs font-semibold text-red-500">▼ {entry.change}</span>
                          ) : (
                            <span className="text-xs text-surface-800/30">—</span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
