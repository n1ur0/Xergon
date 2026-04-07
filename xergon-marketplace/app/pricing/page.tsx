"use client";

import { useState, useEffect } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface AuctionBid {
  provider: string;
  amount: number;
  timestamp: string;
  latencyGuarantee: number;
}

interface ComputeAuction {
  id: string;
  model: string;
  provider: string;
  bidErgPerK: number;
  latencySla: number;
  reputation: number;
  timeRemaining: number;
  status: "active" | "settled" | "expired" | "pending";
  bids: AuctionBid[];
}

interface PriceHistoryPoint {
  timestamp: string;
  price: number;
  model: string;
}

interface AuctionFormData {
  model: string;
  maxBid: string;
  duration: string;
  qualityTier: string;
}

// ── Mock Data ──

const MODELS = [
  "Llama-3.1-70B",
  "Llama-3.1-8B",
  "Mistral-7B-v0.3",
  "Mixtral-8x7B",
  "Qwen-2.5-72B",
  "DeepSeek-V3",
  "Phi-3-Medium",
  "Gemma-2-27B",
];

const QUALITY_TIERS = [
  { value: "standard", label: "Standard", desc: "Best-effort latency" },
  { value: "priority", label: "Priority", desc: "< 500ms P99" },
  { value: "premium", label: "Premium", desc: "< 200ms P99, redundant" },
];

const MOCK_ACTIVE_AUCTIONS: ComputeAuction[] = [
  {
    id: "auc-001",
    model: "Llama-3.1-70B",
    provider: "NeuralForge",
    bidErgPerK: 0.0042,
    latencySla: 320,
    reputation: 97.8,
    timeRemaining: 847,
    status: "active",
    bids: [
      { provider: "GPUHive", amount: 0.0045, timestamp: "2m ago", latencyGuarantee: 350 },
      { provider: "TensorNode", amount: 0.0048, timestamp: "5m ago", latencyGuarantee: 400 },
      { provider: "InferX", amount: 0.0051, timestamp: "8m ago", latencyGuarantee: 380 },
    ],
  },
  {
    id: "auc-002",
    model: "Mistral-7B-v0.3",
    provider: "GPUHive",
    bidErgPerK: 0.0012,
    latencySla: 120,
    reputation: 95.2,
    timeRemaining: 342,
    status: "active",
    bids: [
      { provider: "NeuralForge", amount: 0.0013, timestamp: "1m ago", latencyGuarantee: 130 },
      { provider: "DeepCompute", amount: 0.0015, timestamp: "4m ago", latencyGuarantee: 150 },
    ],
  },
  {
    id: "auc-003",
    model: "Mixtral-8x7B",
    provider: "TensorNode",
    bidErgPerK: 0.0028,
    latencySla: 280,
    reputation: 93.5,
    timeRemaining: 1520,
    status: "active",
    bids: [
      { provider: "GPUHive", amount: 0.0029, timestamp: "3m ago", latencyGuarantee: 300 },
    ],
  },
  {
    id: "auc-004",
    model: "DeepSeek-V3",
    provider: "DeepCompute",
    bidErgPerK: 0.0065,
    latencySla: 450,
    reputation: 91.0,
    timeRemaining: 120,
    status: "active",
    bids: [],
  },
];

const MOCK_HISTORY: ComputeAuction[] = [
  {
    id: "auc-h1",
    model: "Llama-3.1-70B",
    provider: "NeuralForge",
    bidErgPerK: 0.0040,
    latencySla: 300,
    reputation: 97.8,
    timeRemaining: 0,
    status: "settled",
    bids: [
      { provider: "GPUHive", amount: 0.0042, timestamp: "1d ago", latencyGuarantee: 320 },
    ],
  },
  {
    id: "auc-h2",
    model: "Mistral-7B-v0.3",
    provider: "GPUHive",
    bidErgPerK: 0.0011,
    latencySla: 115,
    reputation: 95.2,
    timeRemaining: 0,
    status: "settled",
    bids: [
      { provider: "InferX", amount: 0.0012, timestamp: "2d ago", latencyGuarantee: 125 },
    ],
  },
  {
    id: "auc-h3",
    model: "Phi-3-Medium",
    provider: "TensorNode",
    bidErgPerK: 0.0018,
    latencySla: 200,
    reputation: 93.5,
    timeRemaining: 0,
    status: "settled",
    bids: [],
  },
  {
    id: "auc-h4",
    model: "Gemma-2-27B",
    provider: "DeepCompute",
    bidErgPerK: 0.0035,
    latencySla: 260,
    reputation: 91.0,
    timeRemaining: 0,
    status: "expired",
    bids: [],
  },
];

const MOCK_PRICE_HISTORY: PriceHistoryPoint[] = Array.from({ length: 24 }, (_, i) => ({
  timestamp: `${i}h ago`,
  price: 0.003 + Math.sin(i / 4) * 0.001 + Math.random() * 0.0005,
  model: "Llama-3.1-70B",
}));

// ── Helpers ──

function formatTime(seconds: number): string {
  if (seconds <= 0) return "Expired";
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}m ${s}s`;
}

function reputationColor(rep: number): string {
  if (rep >= 96) return "text-emerald-600";
  if (rep >= 90) return "text-blue-600";
  return "text-yellow-600";
}

function statusBadge(status: ComputeAuction["status"]): { label: string; cls: string } {
  switch (status) {
    case "active":
      return { label: "LIVE", cls: "bg-emerald-100 text-emerald-700" };
    case "settled":
      return { label: "SETTLED", cls: "bg-blue-100 text-blue-700" };
    case "expired":
      return { label: "EXPIRED", cls: "bg-gray-100 text-gray-600" };
    case "pending":
      return { label: "PENDING", cls: "bg-yellow-100 text-yellow-700" };
  }
}

// ── Price Chart Component ──

function PriceChart({ data }: { data: PriceHistoryPoint[] }) {
  const max = Math.max(...data.map(d => d.price));
  const min = Math.min(...data.map(d => d.price));
  const range = max - min || 0.001;
  const width = 600;
  const height = 120;
  const points = data.map((d, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - ((d.price - min) / range) * (height - 20) - 10;
    return `${x},${y}`;
  });

  return (
    <svg viewBox={`0 0 ${width} ${height}`} className="w-full h-32" preserveAspectRatio="none">
      <defs>
        <linearGradient id="chartFill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#6366f1" stopOpacity="0.3" />
          <stop offset="100%" stopColor="#6366f1" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polyline
        fill="url(#chartFill)"
        stroke="none"
        points={`0,${height} ${points.join(" ")} ${width},${height}`}
      />
      <polyline
        fill="none"
        stroke="#6366f1"
        strokeWidth="2"
        points={points.join(" ")}
      />
      {data.map((d, i) => {
        const x = (i / (data.length - 1)) * width;
        const y = height - ((d.price - min) / range) * (height - 20) - 10;
        return (
          <circle key={i} cx={x} cy={y} r="3" fill="#6366f1" />
        );
      })}
    </svg>
  );
}

// ── Auction Card ──

function AuctionCard({ auction }: { auction: ComputeAuction }) {
  const [expanded, setExpanded] = useState(false);
  const badge = statusBadge(auction.status);

  return (
    <div
      className={cn(
        "rounded-xl border p-5 transition-colors cursor-pointer",
        auction.status === "active"
          ? "border-brand-500/30 bg-brand-500/5 hover:border-brand-500/50"
          : "border-surface-200 bg-surface-0"
      )}
      onClick={() => setExpanded(!expanded)}
    >
      <div className="flex items-start justify-between mb-3">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className={cn("inline-block rounded-full px-2.5 py-0.5 text-xs font-bold", badge.cls)}>
              {badge.label}
            </span>
            {auction.status === "active" && (
              <span className="text-xs text-surface-800/40">
                {formatTime(auction.timeRemaining)}
              </span>
            )}
          </div>
          <h3 className="font-semibold text-surface-900">{auction.model}</h3>
          <p className="text-xs text-surface-800/50 mt-0.5">
            by {auction.provider}
          </p>
        </div>
        <div className="text-right">
          <div className="text-lg font-bold text-brand-600 font-mono">
            {auction.bidErgPerK.toFixed(4)}
          </div>
          <div className="text-xs text-surface-800/40">ERG / 1K tokens</div>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-3 text-sm mb-2">
        <div>
          <span className="text-surface-800/40 text-xs block">Latency SLA</span>
          <span className="font-medium text-surface-900">{auction.latencySla}ms</span>
        </div>
        <div>
          <span className="text-surface-800/40 text-xs block">Reputation</span>
          <span className={cn("font-medium", reputationColor(auction.reputation))}>
            {auction.reputation}%
          </span>
        </div>
        <div>
          <span className="text-surface-800/40 text-xs block">Bids</span>
          <span className="font-medium text-surface-900">{auction.bids.length}</span>
        </div>
      </div>

      {expanded && auction.bids.length > 0 && (
        <div className="mt-3 pt-3 border-t border-surface-100">
          <div className="text-xs text-surface-800/40 mb-2 font-medium">Bid History</div>
          <div className="space-y-1.5">
            {auction.bids.map((bid, i) => (
              <div key={i} className="flex items-center justify-between text-sm">
                <span className="text-surface-800/70">{bid.provider}</span>
                <div className="flex items-center gap-3">
                  <span className="text-xs text-surface-800/40">{bid.latencyGuarantee}ms</span>
                  <span className="font-mono text-surface-900 font-medium">
                    {bid.amount.toFixed(4)} ERG
                  </span>
                  <span className="text-xs text-surface-800/30">{bid.timestamp}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Auction Creation Form ──

function AuctionForm({ onSubmit }: { onSubmit: (data: AuctionFormData) => void }) {
  const [form, setForm] = useState<AuctionFormData>({
    model: MODELS[0],
    maxBid: "",
    duration: "15",
    qualityTier: "standard",
  });

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
      <h3 className="font-semibold text-surface-900 mb-4">Create Auction</h3>
      <div className="space-y-4">
        <div>
          <label className="block text-sm text-surface-800/60 mb-1">Model</label>
          <select
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 focus:border-brand-500 focus:outline-none"
            value={form.model}
            onChange={(e) => setForm({ ...form, model: e.target.value })}
          >
            {MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="block text-sm text-surface-800/60 mb-1">Max Bid (ERG / 1K tokens)</label>
          <input
            type="number"
            step="0.0001"
            min="0"
            placeholder="0.0050"
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 focus:border-brand-500 focus:outline-none font-mono"
            value={form.maxBid}
            onChange={(e) => setForm({ ...form, maxBid: e.target.value })}
          />
        </div>
        <div>
          <label className="block text-sm text-surface-800/60 mb-1">Duration (minutes)</label>
          <select
            className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 focus:border-brand-500 focus:outline-none"
            value={form.duration}
            onChange={(e) => setForm({ ...form, duration: e.target.value })}
          >
            <option value="5">5 min</option>
            <option value="15">15 min</option>
            <option value="30">30 min</option>
            <option value="60">1 hour</option>
            <option value="120">2 hours</option>
          </select>
        </div>
        <div>
          <label className="block text-sm text-surface-800/60 mb-2">Quality Tier</label>
          <div className="grid grid-cols-3 gap-2">
            {QUALITY_TIERS.map((tier) => (
              <button
                key={tier.value}
                className={cn(
                  "rounded-lg border px-3 py-2 text-xs text-center transition-colors",
                  form.qualityTier === tier.value
                    ? "border-brand-500 bg-brand-500/10 text-brand-700"
                    : "border-surface-200 bg-surface-0 text-surface-800/60 hover:border-surface-300"
                )}
                onClick={() => setForm({ ...form, qualityTier: tier.value })}
              >
                <div className="font-medium">{tier.label}</div>
                <div className="mt-0.5 opacity-60">{tier.desc}</div>
              </button>
            ))}
          </div>
        </div>
        <button
          className="w-full rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-700 disabled:opacity-50"
          disabled={!form.maxBid || parseFloat(form.maxBid) <= 0}
          onClick={() => onSubmit(form)}
        >
          Start Auction
        </button>
      </div>
    </div>
  );
}

// ── Main Page ──

export default function PricingPage() {
  const [activeAuctions, setActiveAuctions] = useState<ComputeAuction[]>(MOCK_ACTIVE_AUCTIONS);
  const [filterModel, setFilterModel] = useState<string>("all");
  const [filterProvider, setFilterProvider] = useState<string>("all");
  const [filterMaxPrice, setFilterMaxPrice] = useState<string>("");
  const [tab, setTab] = useState<"active" | "history">("active");
  const [priceModel, setPriceModel] = useState<string>("Llama-3.1-70B");

  // Simulate real-time bid updates
  useEffect(() => {
    const interval = setInterval(() => {
      setActiveAuctions((prev) =>
        prev.map((a) => {
          if (a.status !== "active" || a.timeRemaining <= 0) return a;
          const timeRemaining = a.timeRemaining - 1;
          if (timeRemaining <= 0) {
            return { ...a, timeRemaining: 0, status: "settled" as const };
          }
          return { ...a, timeRemaining };
        })
      );
    }, 1000);

    return () => clearInterval(interval);
  }, []);

  // Filter auctions
  const filteredActive = activeAuctions.filter((a) => {
    if (filterModel !== "all" && a.model !== filterModel) return false;
    if (filterProvider !== "all" && a.provider !== filterProvider) return false;
    if (filterMaxPrice && a.bidErgPerK > parseFloat(filterMaxPrice)) return false;
    return true;
  });

  const filteredHistory = MOCK_HISTORY.filter((a) => {
    if (filterModel !== "all" && a.model !== filterModel) return false;
    if (filterProvider !== "all" && a.provider !== filterProvider) return false;
    if (filterMaxPrice && a.bidErgPerK > parseFloat(filterMaxPrice)) return false;
    return true;
  });

  const uniqueProviders = Array.from(
    new Set([...MOCK_ACTIVE_AUCTIONS, ...MOCK_HISTORY].map((a) => a.provider))
  ).sort();

  const handleCreateAuction = (data: AuctionFormData) => {
    const newAuction: ComputeAuction = {
      id: `auc-${Date.now()}`,
      model: data.model,
      provider: "You",
      bidErgPerK: parseFloat(data.maxBid),
      latencySla: data.qualityTier === "premium" ? 200 : data.qualityTier === "priority" ? 500 : 1000,
      reputation: 100,
      timeRemaining: parseInt(data.duration) * 60,
      status: "active",
      bids: [],
    };
    setActiveAuctions((prev) => [newAuction, ...prev]);
  };

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold mb-2">Compute Auction</h1>
      <p className="text-surface-800/60 mb-8">
        GPU providers bid on inference requests in real-time. Get the best price through open competition.
      </p>

      {/* Price Chart */}
      <section className="mb-8">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold">Price History</h2>
          <select
            className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-1.5 text-sm text-surface-900"
            value={priceModel}
            onChange={(e) => setPriceModel(e.target.value)}
          >
            {MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
          <PriceChart data={MOCK_PRICE_HISTORY} />
          <div className="flex justify-between text-xs text-surface-800/40 mt-2">
            <span>24h ago</span>
            <span className="font-medium text-surface-800/60">
              Current: {MOCK_PRICE_HISTORY[MOCK_PRICE_HISTORY.length - 1].price.toFixed(4)} ERG / 1K tokens
            </span>
            <span>Now</span>
          </div>
        </div>
      </section>

      {/* Filters */}
      <section className="mb-6">
        <div className="flex flex-wrap items-center gap-3">
          <select
            className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900"
            value={filterModel}
            onChange={(e) => setFilterModel(e.target.value)}
          >
            <option value="all">All Models</option>
            {MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
          <select
            className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900"
            value={filterProvider}
            onChange={(e) => setFilterProvider(e.target.value)}
          >
            <option value="all">All Providers</option>
            {uniqueProviders.map((p) => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
          <input
            type="number"
            step="0.001"
            placeholder="Max price (ERG/1K)"
            className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-900 w-44 focus:border-brand-500 focus:outline-none"
            value={filterMaxPrice}
            onChange={(e) => setFilterMaxPrice(e.target.value)}
          />
        </div>
      </section>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 border-b border-surface-100">
        {(["active", "history"] as const).map((t) => (
          <button
            key={t}
            className={cn(
              "px-4 py-2.5 text-sm font-medium capitalize transition-colors border-b-2 -mb-px",
              tab === t
                ? "border-brand-600 text-brand-600"
                : "border-transparent text-surface-800/50 hover:text-surface-800/70"
            )}
            onClick={() => setTab(t)}
          >
            {t} {t === "active" ? `(${filteredActive.length})` : `(${filteredHistory.length})`}
          </button>
        ))}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Auction list */}
        <div className="lg:col-span-2">
          {tab === "active" ? (
            filteredActive.length > 0 ? (
              <div className="space-y-4">
                {filteredActive.map((auction) => (
                  <AuctionCard key={auction.id} auction={auction} />
                ))}
              </div>
            ) : (
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
                <p className="text-sm text-surface-800/50">
                  No active auctions match your filters.
                </p>
              </div>
            )
          ) : filteredHistory.length > 0 ? (
            <div className="space-y-4">
              {filteredHistory.map((auction) => (
                <AuctionCard key={auction.id} auction={auction} />
              ))}
            </div>
          ) : (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
              <p className="text-sm text-surface-800/50">
                No auction history found.
              </p>
            </div>
          )}
        </div>

        {/* Sidebar: Auction form + stats */}
        <div className="space-y-6">
          <AuctionForm onSubmit={handleCreateAuction} />

          {/* Market stats */}
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h3 className="font-semibold text-surface-900 mb-3">Market Stats</h3>
            <div className="space-y-3 text-sm">
              <div className="flex justify-between">
                <span className="text-surface-800/50">Active Auctions</span>
                <span className="font-medium text-surface-900">{activeAuctions.filter(a => a.status === "active").length}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Avg. Bid Price</span>
                <span className="font-mono font-medium text-surface-900">
                  {activeAuctions.length > 0
                    ? (activeAuctions.reduce((s, a) => s + a.bidErgPerK, 0) / activeAuctions.length).toFixed(4)
                    : "0.0000"} ERG
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Lowest Bid</span>
                <span className="font-mono font-medium text-emerald-600">
                  {activeAuctions.length > 0
                    ? Math.min(...activeAuctions.map(a => a.bidErgPerK)).toFixed(4)
                    : "0.0000"} ERG
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Avg. Reputation</span>
                <span className="font-medium text-surface-900">
                  {activeAuctions.length > 0
                    ? (activeAuctions.reduce((s, a) => s + a.reputation, 0) / activeAuctions.length).toFixed(1)
                    : "0"}%
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-surface-800/50">Total Providers</span>
                <span className="font-medium text-surface-900">{uniqueProviders.length}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
