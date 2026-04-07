"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { cn } from "@/lib/utils";
import {
  Gavel,
  Clock,
  TrendingUp,
  Filter,
  Plus,
  X,
  ChevronDown,
  ArrowUpDown,
  Flame,
  Trophy,
  CircleDot,
  AlertTriangle,
  History,
  Server,
  Cpu,
  MapPin,
  Zap,
} from "lucide-react";

// ── Types ──────────────────────────────────────────────────────────────────

type AuctionStatus = "active" | "ending_soon" | "won" | "lost" | "ended";

type AuctionType = "gpu_time" | "model_serving";

interface BidHistoryEntry {
  bidder: string;
  amount: number;
  timestamp: Date;
}

interface Auction {
  id: string;
  provider: string;
  providerAvatar: string;
  gpuType: string;
  vram: number;
  region: string;
  auctionType: AuctionType;
  currentBid: number;
  minimumBid: number;
  startingPrice: number;
  bidCount: number;
  status: AuctionStatus;
  endsAt: Date;
  duration: string;
  description: string;
  bidHistory: BidHistoryEntry[];
  isHot: boolean;
}

interface UserBid {
  auctionId: string;
  auctionTitle: string;
  gpuType: string;
  yourBid: number;
  currentBid: number;
  status: AuctionStatus;
  endsAt: Date;
  bidPlacedAt: Date;
}

interface AuctionFilters {
  gpuType: string;
  minVram: number | null;
  maxPrice: number | null;
  region: string;
  auctionType: string;
  status: string;
  sortBy: "ending_soon" | "price_low" | "price_high" | "most_bids" | "newest";
}

// ── Mock Data ──────────────────────────────────────────────────────────────

const NOW = new Date();

function futureDate(hours: number, minutes = 0): Date {
  const d = new Date(NOW.getTime());
  d.setHours(d.getHours() + hours, d.getMinutes() + minutes);
  return d;
}

function pastDate(hours: number): Date {
  const d = new Date(NOW.getTime());
  d.setHours(d.getHours() - hours);
  return d;
}

const MOCK_AUCTIONS: Auction[] = [
  {
    id: "auc-001",
    provider: "NeuralForge",
    providerAvatar: "NF",
    gpuType: "NVIDIA A100",
    vram: 80,
    region: "US-East",
    auctionType: "gpu_time",
    currentBid: 12.5,
    minimumBid: 13.0,
    startingPrice: 5.0,
    bidCount: 14,
    status: "active",
    endsAt: futureDate(2, 30),
    duration: "24h slot",
    description: "80GB A100 GPU time for training or inference. High-bandwidth HBM2e memory.",
    bidHistory: [
      { bidder: "0x3f2a...b8c1", amount: 5.0, timestamp: pastDate(22) },
      { bidder: "0x7e1d...a4f2", amount: 6.5, timestamp: pastDate(20) },
      { bidder: "0x9c4b...d2e8", amount: 8.0, timestamp: pastDate(18) },
      { bidder: "0x1a6f...c5d3", amount: 9.5, timestamp: pastDate(15) },
      { bidder: "0x5d8e...f1a7", amount: 10.5, timestamp: pastDate(12) },
      { bidder: "0x2b7c...e9f4", amount: 11.0, timestamp: pastDate(10) },
      { bidder: "0x8f3a...b2d6", amount: 11.5, timestamp: pastDate(8) },
      { bidder: "0x4e9d...c1a5", amount: 12.0, timestamp: pastDate(5) },
      { bidder: "0x6c2f...a8e3", amount: 12.5, timestamp: pastDate(2) },
    ],
    isHot: true,
  },
  {
    id: "auc-002",
    provider: "ComputeHive",
    providerAvatar: "CH",
    gpuType: "NVIDIA H100",
    vram: 80,
    region: "EU-West",
    auctionType: "gpu_time",
    currentBid: 24.0,
    minimumBid: 25.0,
    startingPrice: 10.0,
    bidCount: 22,
    status: "ending_soon",
    endsAt: futureDate(0, 15),
    duration: "12h slot",
    description: "Next-gen H100 with Transformer Engine. Ideal for LLM fine-tuning.",
    bidHistory: [
      { bidder: "0x1a2b...c3d4", amount: 10.0, timestamp: pastDate(11) },
      { bidder: "0x5e6f...a7b8", amount: 14.0, timestamp: pastDate(9) },
      { bidder: "0x9c0d...e1f2", amount: 17.0, timestamp: pastDate(7) },
      { bidder: "0x3a4b...c5d6", amount: 19.5, timestamp: pastDate(5) },
      { bidder: "0x7e8f...a1b2", amount: 21.0, timestamp: pastDate(3) },
      { bidder: "0x2c3d...e4f5", amount: 22.5, timestamp: pastDate(1) },
      { bidder: "0x6a7b...c8d9", amount: 24.0, timestamp: pastDate(0.5) },
    ],
    isHot: true,
  },
  {
    id: "auc-003",
    provider: "DeepOps",
    providerAvatar: "DO",
    gpuType: "NVIDIA RTX 4090",
    vram: 24,
    region: "AP-Southeast",
    auctionType: "gpu_time",
    currentBid: 4.2,
    minimumBid: 4.5,
    startingPrice: 1.0,
    bidCount: 8,
    status: "active",
    endsAt: futureDate(8, 0),
    duration: "48h slot",
    description: "Consumer-grade powerhouse for inference workloads. 24GB GDDR6X.",
    bidHistory: [
      { bidder: "0xab12...cd34", amount: 1.0, timestamp: pastDate(40) },
      { bidder: "0xef56...gh78", amount: 1.8, timestamp: pastDate(36) },
      { bidder: "0xij90...kl12", amount: 2.5, timestamp: pastDate(30) },
      { bidder: "0xmn34...op56", amount: 3.0, timestamp: pastDate(24) },
      { bidder: "0xqr78...st90", amount: 3.5, timestamp: pastDate(18) },
      { bidder: "0xuv12...wx34", amount: 4.2, timestamp: pastDate(10) },
    ],
    isHot: false,
  },
  {
    id: "auc-004",
    provider: "ModelServe Co",
    providerAvatar: "MS",
    gpuType: "NVIDIA A6000",
    vram: 48,
    region: "US-West",
    auctionType: "model_serving",
    currentBid: 18.75,
    minimumBid: 19.5,
    startingPrice: 8.0,
    bidCount: 11,
    status: "active",
    endsAt: futureDate(5, 45),
    duration: "7 days",
    description: "Dedicated model serving slot. Run your fine-tuned LLM 24/7 for a week.",
    bidHistory: [
      { bidder: "0x1a2b...c3d4", amount: 8.0, timestamp: pastDate(6 * 24) },
      { bidder: "0x5e6f...a7b8", amount: 10.0, timestamp: pastDate(5 * 24) },
      { bidder: "0x9c0d...e1f2", amount: 12.5, timestamp: pastDate(4 * 24) },
      { bidder: "0x3a4b...c5d6", amount: 14.0, timestamp: pastDate(3 * 24) },
      { bidder: "0x7e8f...a1b2", amount: 16.0, timestamp: pastDate(2 * 24) },
      { bidder: "0x2c3d...e4f5", amount: 17.5, timestamp: pastDate(24) },
      { bidder: "0x6a7b...c8d9", amount: 18.75, timestamp: pastDate(8) },
    ],
    isHot: false,
  },
  {
    id: "auc-005",
    provider: "GPU Pool",
    providerAvatar: "GP",
    gpuType: "NVIDIA A100",
    vram: 40,
    region: "EU-Central",
    auctionType: "gpu_time",
    currentBid: 8.0,
    minimumBid: 8.5,
    startingPrice: 3.0,
    bidCount: 6,
    status: "active",
    endsAt: futureDate(14, 20),
    duration: "24h slot",
    description: "40GB A100 variant. Great for medium model training and inference.",
    bidHistory: [
      { bidder: "0xaa11...bb22", amount: 3.0, timestamp: pastDate(20) },
      { bidder: "0xcc33...dd44", amount: 5.0, timestamp: pastDate(16) },
      { bidder: "0xee55...ff66", amount: 6.0, timestamp: pastDate(12) },
      { bidder: "0xgg77...hh88", amount: 7.0, timestamp: pastDate(8) },
      { bidder: "0xii99...jj00", amount: 8.0, timestamp: pastDate(4) },
    ],
    isHot: false,
  },
  {
    id: "auc-006",
    provider: "ServeNet",
    providerAvatar: "SN",
    gpuType: "NVIDIA L40S",
    vram: 48,
    region: "US-East",
    auctionType: "model_serving",
    currentBid: 15.0,
    minimumBid: 15.5,
    startingPrice: 6.0,
    bidCount: 9,
    status: "ending_soon",
    endsAt: futureDate(0, 42),
    duration: "30 days",
    description: "Month-long dedicated serving. L40S optimized for inference throughput.",
    bidHistory: [
      { bidder: "0x11aa...22bb", amount: 6.0, timestamp: pastDate(28 * 24) },
      { bidder: "0x33cc...44dd", amount: 8.5, timestamp: pastDate(24 * 24) },
      { bidder: "0x55ee...66ff", amount: 10.0, timestamp: pastDate(20 * 24) },
      { bidder: "0x77gg...88hh", amount: 12.0, timestamp: pastDate(14 * 24) },
      { bidder: "0x99ii...00jj", amount: 13.5, timestamp: pastDate(7 * 24) },
      { bidder: "0x11kk...22ll", amount: 15.0, timestamp: pastDate(2) },
    ],
    isHot: true,
  },
  {
    id: "auc-007",
    provider: "TensorDock",
    providerAvatar: "TD",
    gpuType: "NVIDIA RTX 3090",
    vram: 24,
    region: "AP-Northeast",
    auctionType: "gpu_time",
    currentBid: 2.8,
    minimumBid: 3.0,
    startingPrice: 0.5,
    bidCount: 5,
    status: "active",
    endsAt: futureDate(22, 10),
    duration: "72h slot",
    description: "Budget-friendly 24GB GPU. Perfect for experimentation and prototyping.",
    bidHistory: [
      { bidder: "0x1aaa...2bbb", amount: 0.5, timestamp: pastDate(68) },
      { bidder: "0x3ccc...4ddd", amount: 1.2, timestamp: pastDate(50) },
      { bidder: "0x5eee...6fff", amount: 1.8, timestamp: pastDate(36) },
      { bidder: "0x7ggg...8hhh", amount: 2.3, timestamp: pastDate(20) },
      { bidder: "0x9iii...0jjj", amount: 2.8, timestamp: pastDate(10) },
    ],
    isHot: false,
  },
  {
    id: "auc-008",
    provider: "NeuralForge",
    providerAvatar: "NF",
    gpuType: "NVIDIA H100",
    vram: 80,
    region: "US-East",
    auctionType: "model_serving",
    currentBid: 45.0,
    minimumBid: 46.0,
    startingPrice: 20.0,
    bidCount: 18,
    status: "active",
    endsAt: futureDate(11, 0),
    duration: "14 days",
    description: "Premium H100 serving slot. Two weeks of dedicated next-gen compute.",
    bidHistory: [
      { bidder: "0xaaa1...bbb2", amount: 20.0, timestamp: pastDate(13 * 24) },
      { bidder: "0xccc3...ddd4", amount: 25.0, timestamp: pastDate(11 * 24) },
      { bidder: "0xeee5...fff6", amount: 30.0, timestamp: pastDate(9 * 24) },
      { bidder: "0xggg7...hhh8", amount: 35.0, timestamp: pastDate(7 * 24) },
      { bidder: "0xiii9...jjj0", amount: 38.0, timestamp: pastDate(5 * 24) },
      { bidder: "0xkkk1...lll2", amount: 40.0, timestamp: pastDate(3 * 24) },
      { bidder: "0xmmm3...nnn4", amount: 42.0, timestamp: pastDate(24) },
      { bidder: "0xooo5...ppp6", amount: 45.0, timestamp: pastDate(6) },
    ],
    isHot: true,
  },
];

const MOCK_MY_BIDS: UserBid[] = [
  {
    auctionId: "auc-001",
    auctionTitle: "NVIDIA A100 80GB",
    gpuType: "NVIDIA A100",
    yourBid: 12.0,
    currentBid: 12.5,
    status: "lost",
    endsAt: futureDate(2, 30),
    bidPlacedAt: pastDate(5),
  },
  {
    auctionId: "auc-003",
    auctionTitle: "NVIDIA RTX 4090 24GB",
    gpuType: "NVIDIA RTX 4090",
    yourBid: 4.2,
    currentBid: 4.2,
    status: "won",
    endsAt: futureDate(8, 0),
    bidPlacedAt: pastDate(10),
  },
  {
    auctionId: "auc-005",
    auctionTitle: "NVIDIA A100 40GB",
    gpuType: "NVIDIA A100",
    yourBid: 8.0,
    currentBid: 8.0,
    status: "won",
    endsAt: futureDate(14, 20),
    bidPlacedAt: pastDate(4),
  },
  {
    auctionId: "auc-008",
    auctionTitle: "NVIDIA H100 80GB Serving",
    gpuType: "NVIDIA H100",
    yourBid: 42.0,
    currentBid: 45.0,
    status: "lost",
    endsAt: futureDate(11, 0),
    bidPlacedAt: pastDate(24),
  },
];

const GPU_TYPES = ["All", "NVIDIA A100", "NVIDIA H100", "NVIDIA RTX 4090", "NVIDIA A6000", "NVIDIA L40S", "NVIDIA RTX 3090"];
const REGIONS = ["All", "US-East", "US-West", "EU-West", "EU-Central", "AP-Southeast", "AP-Northeast"];

// ── Helper Components ──────────────────────────────────────────────────────

function StatusBadge({ status }: { status: AuctionStatus }) {
  const config: Record<AuctionStatus, { label: string; className: string; icon: React.ReactNode }> = {
    active: {
      label: "Active",
      className: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
      icon: <CircleDot className="w-3 h-3" />,
    },
    ending_soon: {
      label: "Ending Soon",
      className: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
      icon: <AlertTriangle className="w-3 h-3" />,
    },
    won: {
      label: "Won",
      className: "bg-brand-100 text-brand-700 dark:bg-brand-900/30 dark:text-brand-400",
      icon: <Trophy className="w-3 h-3" />,
    },
    lost: {
      label: "Outbid",
      className: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
      icon: <TrendingUp className="w-3 h-3" />,
    },
    ended: {
      label: "Ended",
      className: "bg-surface-200 text-surface-800 dark:bg-surface-200 dark:text-surface-800",
      icon: <Clock className="w-3 h-3" />,
    },
  };

  const c = config[status];
  return (
    <span className={cn("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium", c.className)}>
      {c.icon}
      {c.label}
    </span>
  );
}

function CountdownTimer({ endsAt }: { endsAt: Date }) {
  const [timeLeft, setTimeLeft] = useState(getTimeLeft(endsAt));

  function getTimeLeft(target: Date) {
    const diff = target.getTime() - Date.now();
    if (diff <= 0) return { hours: 0, minutes: 0, seconds: 0, total: 0 };
    return {
      hours: Math.floor(diff / 3600000),
      minutes: Math.floor((diff % 3600000) / 60000),
      seconds: Math.floor((diff % 60000) / 1000),
      total: diff,
    };
  }

  useEffect(() => {
    const interval = setInterval(() => {
      setTimeLeft(getTimeLeft(endsAt));
    }, 1000);
    return () => clearInterval(interval);
  }, [endsAt]);

  const isUrgent = timeLeft.total < 3600000; // < 1 hour
  const pad = (n: number) => String(n).padStart(2, "0");

  return (
    <div className={cn("flex items-center gap-1 font-mono text-sm", isUrgent ? "text-red-500 dark:text-red-400" : "text-surface-800/70 dark:text-surface-800/60")}>
      <Clock className={cn("w-3.5 h-3.5", isUrgent && "animate-pulse")} />
      <span>
        {pad(timeLeft.hours)}:{pad(timeLeft.minutes)}:{pad(timeLeft.seconds)}
      </span>
    </div>
  );
}

function BidHistoryChart({ bidHistory }: { bidHistory: BidHistoryEntry[] }) {
  if (bidHistory.length === 0) return null;

  const maxAmount = Math.max(...bidHistory.map((b) => b.amount));
  const chartHeight = 120;
  const barWidth = Math.max(8, Math.min(24, 240 / bidHistory.length));
  const gap = 4;

  return (
    <div className="mt-3">
      <p className="text-xs font-medium text-surface-800/60 dark:text-surface-800/50 mb-2 uppercase tracking-wide">
        Bid History
      </p>
      <svg
        viewBox={`0 0 ${bidHistory.length * (barWidth + gap) + 20} ${chartHeight + 30}`}
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Y-axis labels */}
        <text x="0" y="12" className="fill-surface-800/40 dark:fill-surface-800/40" fontSize="8" fontFamily="monospace">
          {maxAmount.toFixed(1)}
        </text>
        <text x="0" y={chartHeight + 6} className="fill-surface-800/40 dark:fill-surface-800/40" fontSize="8" fontFamily="monospace">
          0
        </text>

        {/* Grid line */}
        <line x1="30" y1={chartHeight + 12} x2={bidHistory.length * (barWidth + gap) + 20} y2={chartHeight + 12} stroke="currentColor" className="stroke-surface-200 dark:stroke-surface-200" strokeWidth="0.5" />

        {/* Bars */}
        {bidHistory.map((bid, i) => {
          const barHeight = (bid.amount / maxAmount) * chartHeight;
          const x = 35 + i * (barWidth + gap);
          const y = chartHeight + 12 - barHeight;
          const isLast = i === bidHistory.length - 1;

          return (
            <g key={i}>
              <rect
                x={x}
                y={y}
                width={barWidth}
                height={barHeight}
                rx={2}
                className={cn(
                  isLast
                    ? "fill-brand-500 dark:fill-brand-400"
                    : "fill-brand-200 dark:fill-brand-800"
                )}
                opacity={isLast ? 1 : 0.7}
              />
              {/* Amount label on last bar */}
              {isLast && (
                <text
                  x={x + barWidth / 2}
                  y={y - 4}
                  textAnchor="middle"
                  className="fill-brand-600 dark:fill-brand-400"
                  fontSize="7"
                  fontWeight="600"
                  fontFamily="monospace"
                >
                  {bid.amount} ERG
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ── Auction Card ───────────────────────────────────────────────────────────

function AuctionCard({
  auction,
  onBid,
  onSelect,
}: {
  auction: Auction;
  onBid: (auction: Auction) => void;
  onSelect: (auction: Auction) => void;
}) {
  return (
    <div
      className={cn(
        "relative rounded-xl border bg-surface-0 p-4 transition-all hover:shadow-md cursor-pointer group",
        "border-surface-200 dark:border-surface-200",
        auction.isHot && "ring-1 ring-amber-300/50 dark:ring-amber-500/30"
      )}
      onClick={() => onSelect(auction)}
    >
      {/* Hot badge */}
      {auction.isHot && (
        <div className="absolute -top-2 -right-2 bg-amber-500 text-white rounded-full px-2 py-0.5 text-xs font-bold flex items-center gap-1 shadow-sm">
          <Flame className="w-3 h-3" />
          Hot
        </div>
      )}

      {/* Header */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2.5">
          <div className="w-9 h-9 rounded-lg bg-brand-100 dark:bg-brand-900/30 flex items-center justify-center text-brand-700 dark:text-brand-400 text-xs font-bold">
            {auction.providerAvatar}
          </div>
          <div>
            <p className="text-sm font-semibold text-surface-900 dark:text-surface-900">{auction.provider}</p>
            <div className="flex items-center gap-1 text-xs text-surface-800/50 dark:text-surface-800/50">
              <MapPin className="w-3 h-3" />
              {auction.region}
            </div>
          </div>
        </div>
        <StatusBadge status={auction.status} />
      </div>

      {/* GPU Info */}
      <div className="mb-3">
        <div className="flex items-center gap-1.5 mb-1">
          <Cpu className="w-4 h-4 text-brand-500" />
          <h3 className="text-base font-bold text-surface-900 dark:text-surface-900">{auction.gpuType}</h3>
        </div>
        <p className="text-xs text-surface-800/60 dark:text-surface-800/50 line-clamp-2">{auction.description}</p>
      </div>

      {/* Specs Row */}
      <div className="flex items-center gap-3 mb-3 text-xs text-surface-800/70 dark:text-surface-800/60">
        <span className="flex items-center gap-1 bg-surface-100 dark:bg-surface-100 rounded-md px-2 py-1">
          <Server className="w-3 h-3" />
          {auction.vram}GB VRAM
        </span>
        <span className="flex items-center gap-1 bg-surface-100 dark:bg-surface-100 rounded-md px-2 py-1">
          <Zap className="w-3 h-3" />
          {auction.auctionType === "gpu_time" ? "GPU Time" : "Serving Slot"}
        </span>
        <span className="flex items-center gap-1 bg-surface-100 dark:bg-surface-100 rounded-md px-2 py-1">
          <Clock className="w-3 h-3" />
          {auction.duration}
        </span>
      </div>

      {/* Bid History Chart (compact) */}
      <BidHistoryChart bidHistory={auction.bidHistory} />

      {/* Price & Timer */}
      <div className="flex items-end justify-between mt-3 pt-3 border-t border-surface-100 dark:border-surface-100">
        <div>
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50">Current Bid</p>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-900">
            {auction.currentBid} <span className="text-sm font-normal text-surface-800/50">ERG</span>
          </p>
          <p className="text-xs text-surface-800/40 dark:text-surface-800/40">
            Min. next: {auction.minimumBid} ERG · {auction.bidCount} bids
          </p>
        </div>
        <div className="flex flex-col items-end gap-2">
          <CountdownTimer endsAt={auction.endsAt} />
          <button
            onClick={(e) => {
              e.stopPropagation();
              onBid(auction);
            }}
            className={cn(
              "px-4 py-1.5 rounded-lg text-sm font-semibold transition-all",
              "bg-brand-600 text-white hover:bg-brand-700 active:scale-95",
              "shadow-sm hover:shadow-md"
            )}
          >
            Place Bid
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Bid Modal ──────────────────────────────────────────────────────────────

function BidModal({
  auction,
  isOpen,
  onClose,
  onConfirm,
}: {
  auction: Auction | null;
  isOpen: boolean;
  onClose: () => void;
  onConfirm: (auctionId: string, amount: number) => void;
}) {
  const [bidAmount, setBidAmount] = useState("");
  const [error, setError] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (auction) {
      setBidAmount(auction.minimumBid.toString());
      setError("");
      setIsSubmitting(false);
    }
  }, [auction]);

  if (!isOpen || !auction) return null;

  function handleSubmit() {
    if (!auction) return;
    const amount = parseFloat(bidAmount);
    if (isNaN(amount) || amount <= 0) {
      setError("Enter a valid bid amount");
      return;
    }
    if (amount < auction.minimumBid) {
      setError(`Minimum bid is ${auction.minimumBid} ERG`);
      return;
    }
    setIsSubmitting(true);
    // Simulate API call
    setTimeout(() => {
      onConfirm(auction.id, amount);
      setIsSubmitting(false);
      onClose();
    }, 800);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      {/* Modal */}
      <div
        className="relative w-full max-w-md rounded-2xl bg-surface-0 dark:bg-surface-50 p-6 shadow-xl border border-surface-200 dark:border-surface-200"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 p-1 rounded-lg hover:bg-surface-100 dark:hover:bg-surface-100 transition-colors"
        >
          <X className="w-5 h-5 text-surface-800/60" />
        </button>

        <h2 className="text-lg font-bold text-surface-900 dark:text-surface-900 mb-1">Place Bid</h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-800/50 mb-4">
          {auction.gpuType} — {auction.vram}GB · {auction.provider}
        </p>

        {/* Current bid info */}
        <div className="bg-surface-100 dark:bg-surface-100 rounded-lg p-3 mb-4">
          <div className="flex justify-between text-sm">
            <span className="text-surface-800/60">Current Bid</span>
            <span className="font-semibold text-surface-900 dark:text-surface-900">{auction.currentBid} ERG</span>
          </div>
          <div className="flex justify-between text-sm mt-1">
            <span className="text-surface-800/60">Minimum Next Bid</span>
            <span className="font-semibold text-brand-600 dark:text-brand-500">{auction.minimumBid} ERG</span>
          </div>
          <div className="flex justify-between text-sm mt-1">
            <span className="text-surface-800/60">Total Bids</span>
            <span className="font-semibold text-surface-900 dark:text-surface-900">{auction.bidCount}</span>
          </div>
        </div>

        {/* Bid input */}
        <div className="mb-4">
          <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">
            Your Bid (ERG)
          </label>
          <div className="relative">
            <input
              type="number"
              step="0.1"
              min={auction.minimumBid}
              value={bidAmount}
              onChange={(e) => {
                setBidAmount(e.target.value);
                setError("");
              }}
              className={cn(
                "w-full rounded-lg border bg-surface-0 px-4 py-2.5 text-sm font-mono",
                "text-surface-900 dark:text-surface-900",
                "border-surface-200 dark:border-surface-200",
                "focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent",
                "transition-all",
                error && "border-red-400 focus:ring-red-400"
              )}
              placeholder={`${auction.minimumBid} ERG or more`}
            />
            <span className="absolute right-3 top-1/2 -translate-y-1/2 text-sm text-surface-800/40 font-medium">
              ERG
            </span>
          </div>
          {error && <p className="text-xs text-red-500 mt-1">{error}</p>}
        </div>

        {/* Quick bid buttons */}
        <div className="flex gap-2 mb-4">
          {[auction.minimumBid, auction.minimumBid + 1, auction.minimumBid + 5].map((val) => (
            <button
              key={val}
              onClick={() => {
                setBidAmount(val.toString());
                setError("");
              }}
              className={cn(
                "flex-1 rounded-lg border border-surface-200 dark:border-surface-200 px-3 py-1.5 text-xs font-medium transition-colors",
                "hover:bg-brand-50 hover:border-brand-300 hover:text-brand-700",
                "dark:hover:bg-brand-900/20 dark:hover:border-brand-700 dark:hover:text-brand-400",
                "text-surface-800/70 dark:text-surface-800/60"
              )}
            >
              {val} ERG
            </button>
          ))}
        </div>

        {/* Submit */}
        <button
          onClick={handleSubmit}
          disabled={isSubmitting}
          className={cn(
            "w-full rounded-lg py-2.5 text-sm font-semibold transition-all",
            "bg-brand-600 text-white hover:bg-brand-700 active:scale-[0.98]",
            "shadow-sm hover:shadow-md",
            isSubmitting && "opacity-70 cursor-not-allowed"
          )}
        >
          {isSubmitting ? (
            <span className="flex items-center justify-center gap-2">
              <svg className="animate-spin w-4 h-4" viewBox="0 0 24 24" fill="none">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              Confirming...
            </span>
          ) : (
            "Confirm Bid"
          )}
        </button>

        <p className="text-xs text-center text-surface-800/40 mt-3">
          Bids are placed on the Ergo blockchain and are non-reversible.
        </p>
      </div>
    </div>
  );
}

// ── Create Auction Modal ───────────────────────────────────────────────────

function CreateAuctionModal({
  isOpen,
  onClose,
  onCreate,
}: {
  isOpen: boolean;
  onClose: () => void;
  onCreate: (data: {
    gpuType: string;
    vram: number;
    region: string;
    auctionType: AuctionType;
    startingPrice: number;
    duration: string;
    description: string;
  }) => void;
}) {
  const [gpuType, setGpuType] = useState("NVIDIA A100");
  const [vram, setVram] = useState("80");
  const [region, setRegion] = useState("US-East");
  const [auctionType, setAuctionType] = useState<AuctionType>("gpu_time");
  const [startingPrice, setStartingPrice] = useState("5.0");
  const [duration, setDuration] = useState("24h");
  const [description, setDescription] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  if (!isOpen) return null;

  function handleSubmit() {
    if (!startingPrice || parseFloat(startingPrice) <= 0) return;
    setIsSubmitting(true);
    setTimeout(() => {
      onCreate({
        gpuType,
        vram: parseInt(vram) || 24,
        region,
        auctionType,
        startingPrice: parseFloat(startingPrice),
        duration,
        description: description || `${gpuType} ${vram}GB compute auction`,
      });
      setIsSubmitting(false);
      onClose();
    }, 800);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      <div
        className="relative w-full max-w-lg rounded-2xl bg-surface-0 dark:bg-surface-50 p-6 shadow-xl border border-surface-200 dark:border-surface-200 max-h-[90vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 p-1 rounded-lg hover:bg-surface-100 dark:hover:bg-surface-100 transition-colors"
        >
          <X className="w-5 h-5 text-surface-800/60" />
        </button>

        <h2 className="text-lg font-bold text-surface-900 dark:text-surface-900 mb-1">Create Auction</h2>
        <p className="text-sm text-surface-800/60 dark:text-surface-800/50 mb-5">
          List your compute resources for auction on Xergon Network.
        </p>

        <div className="space-y-4">
          {/* GPU Type */}
          <div>
            <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">GPU Type</label>
            <select
              value={gpuType}
              onChange={(e) => setGpuType(e.target.value)}
              className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
            >
              {GPU_TYPES.filter((t) => t !== "All").map((t) => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
          </div>

          {/* VRAM & Region */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">VRAM (GB)</label>
              <input
                type="number"
                value={vram}
                onChange={(e) => setVram(e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-brand-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">Region</label>
              <select
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                {REGIONS.filter((r) => r !== "All").map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
            </div>
          </div>

          {/* Auction Type */}
          <div>
            <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">Auction Type</label>
            <div className="flex gap-2">
              {(["gpu_time", "model_serving"] as const).map((t) => (
                <button
                  key={t}
                  onClick={() => setAuctionType(t)}
                  className={cn(
                    "flex-1 rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
                    auctionType === t
                      ? "border-brand-500 bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-400 dark:border-brand-700"
                      : "border-surface-200 dark:border-surface-200 text-surface-800/60 hover:bg-surface-100 dark:hover:bg-surface-100"
                  )}
                >
                  {t === "gpu_time" ? "GPU Time" : "Serving Slot"}
                </button>
              ))}
            </div>
          </div>

          {/* Starting Price & Duration */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">Starting Price (ERG)</label>
              <input
                type="number"
                step="0.1"
                min="0.1"
                value={startingPrice}
                onChange={(e) => setStartingPrice(e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-brand-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">Duration</label>
              <select
                value={duration}
                onChange={(e) => setDuration(e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                <option value="6h">6 hours</option>
                <option value="12h">12 hours</option>
                <option value="24h">24 hours</option>
                <option value="48h">48 hours</option>
                <option value="7d">7 days</option>
                <option value="14d">14 days</option>
                <option value="30d">30 days</option>
              </select>
            </div>
          </div>

          {/* Description */}
          <div>
            <label className="block text-sm font-medium text-surface-900 dark:text-surface-900 mb-1.5">Description</label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="Describe your compute offering..."
              className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500 resize-none"
            />
          </div>
        </div>

        <button
          onClick={handleSubmit}
          disabled={isSubmitting}
          className={cn(
            "w-full mt-5 rounded-lg py-2.5 text-sm font-semibold transition-all",
            "bg-brand-600 text-white hover:bg-brand-700 active:scale-[0.98]",
            "shadow-sm hover:shadow-md",
            isSubmitting && "opacity-70 cursor-not-allowed"
          )}
        >
          {isSubmitting ? "Creating..." : "Create Auction"}
        </button>
      </div>
    </div>
  );
}

// ── Filters Bar ────────────────────────────────────────────────────────────

function FiltersBar({
  filters,
  onFiltersChange,
  totalResults,
}: {
  filters: AuctionFilters;
  onFiltersChange: (f: AuctionFilters) => void;
  totalResults: number;
}) {
  const [showFilters, setShowFilters] = useState(false);

  function updateFilter<K extends keyof AuctionFilters>(key: K, value: AuctionFilters[K]) {
    onFiltersChange({ ...filters, [key]: value });
  }

  function clearFilters() {
    onFiltersChange({
      gpuType: "All",
      minVram: null,
      maxPrice: null,
      region: "All",
      auctionType: "All",
      status: "All",
      sortBy: "ending_soon",
    });
  }

  const hasActiveFilters = filters.gpuType !== "All" || filters.minVram !== null || filters.maxPrice !== null || filters.region !== "All" || filters.auctionType !== "All" || filters.status !== "All";

  return (
    <div className="mb-6">
      {/* Top bar */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setShowFilters(!showFilters)}
            className={cn(
              "flex items-center gap-1.5 rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
              "border-surface-200 dark:border-surface-200",
              "text-surface-800/70 dark:text-surface-800/60",
              "hover:bg-surface-100 dark:hover:bg-surface-100",
              showFilters && "bg-brand-50 border-brand-300 text-brand-700 dark:bg-brand-900/20 dark:border-brand-700 dark:text-brand-400"
            )}
          >
            <Filter className="w-4 h-4" />
            Filters
            {hasActiveFilters && (
              <span className="rounded-full bg-brand-500 text-white w-4 h-4 text-xs flex items-center justify-center">!</span>
            )}
          </button>

          {/* Sort dropdown */}
          <div className="relative">
            <select
              value={filters.sortBy}
              onChange={(e) => updateFilter("sortBy", e.target.value as AuctionFilters["sortBy"])}
              className="appearance-none rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 pl-3 pr-8 py-2 text-sm font-medium text-surface-800/70 dark:text-surface-800/60 focus:outline-none focus:ring-2 focus:ring-brand-500 cursor-pointer"
            >
              <option value="ending_soon">Ending Soon</option>
              <option value="price_low">Price: Low to High</option>
              <option value="price_high">Price: High to Low</option>
              <option value="most_bids">Most Bids</option>
              <option value="newest">Newest</option>
            </select>
            <ArrowUpDown className="absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-surface-800/40 pointer-events-none" />
          </div>
        </div>

        <p className="text-sm text-surface-800/50 dark:text-surface-800/50">
          {totalResults} auction{totalResults !== 1 ? "s" : ""}
        </p>
      </div>

      {/* Expandable filters */}
      {showFilters && (
        <div className="mt-3 p-4 rounded-xl border border-surface-200 dark:border-surface-200 bg-surface-0 dark:bg-surface-50 space-y-4 animate-fade-in">
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
            {/* GPU Type */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">GPU Type</label>
              <select
                value={filters.gpuType}
                onChange={(e) => updateFilter("gpuType", e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                {GPU_TYPES.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>

            {/* Region */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Region</label>
              <select
                value={filters.region}
                onChange={(e) => updateFilter("region", e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                {REGIONS.map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
            </div>

            {/* Auction Type */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Type</label>
              <select
                value={filters.auctionType}
                onChange={(e) => updateFilter("auctionType", e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                <option value="All">All Types</option>
                <option value="gpu_time">GPU Time</option>
                <option value="model_serving">Serving Slot</option>
              </select>
            </div>

            {/* Status */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Status</label>
              <select
                value={filters.status}
                onChange={(e) => updateFilter("status", e.target.value)}
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500"
              >
                <option value="All">All Status</option>
                <option value="active">Active</option>
                <option value="ending_soon">Ending Soon</option>
              </select>
            </div>

            {/* Min VRAM */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Min VRAM (GB)</label>
              <input
                type="number"
                value={filters.minVram ?? ""}
                onChange={(e) => updateFilter("minVram", e.target.value ? Number(e.target.value) : null)}
                placeholder="Any"
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-brand-500"
              />
            </div>

            {/* Max Price */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1">Max Price (ERG)</label>
              <input
                type="number"
                step="0.1"
                value={filters.maxPrice ?? ""}
                onChange={(e) => updateFilter("maxPrice", e.target.value ? Number(e.target.value) : null)}
                placeholder="Any"
                className="w-full rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0 px-2 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-brand-500"
              />
            </div>
          </div>

          {hasActiveFilters && (
            <button
              onClick={clearFilters}
              className="text-xs text-brand-600 dark:text-brand-400 hover:underline"
            >
              Clear all filters
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ── My Bids Section ────────────────────────────────────────────────────────

function MyBidsSection({ bids }: { bids: UserBid[] }) {
  if (bids.length === 0) {
    return (
      <div className="text-center py-12">
        <History className="w-12 h-12 text-surface-800/20 dark:text-surface-800/20 mx-auto mb-3" />
        <p className="text-sm text-surface-800/50 dark:text-surface-800/50">You haven&apos;t placed any bids yet.</p>
        <p className="text-xs text-surface-800/30 dark:text-surface-800/30 mt-1">Browse active auctions above to get started.</p>
      </div>
    );
  }

  const activeBids = bids.filter((b) => b.status === "won");
  const outbidBids = bids.filter((b) => b.status === "lost");

  return (
    <div className="space-y-4">
      {activeBids.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold text-emerald-600 dark:text-emerald-400 mb-2 flex items-center gap-1.5">
            <Trophy className="w-4 h-4" />
            Winning ({activeBids.length})
          </h3>
          <div className="space-y-2">
            {activeBids.map((bid) => (
              <BidRow key={bid.auctionId} bid={bid} />
            ))}
          </div>
        </div>
      )}

      {outbidBids.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold text-red-500 dark:text-red-400 mb-2 flex items-center gap-1.5">
            <AlertTriangle className="w-4 h-4" />
            Outbid ({outbidBids.length})
          </h3>
          <div className="space-y-2">
            {outbidBids.map((bid) => (
              <BidRow key={bid.auctionId} bid={bid} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function BidRow({ bid }: { bid: UserBid }) {
  return (
    <div className="flex items-center justify-between p-3 rounded-lg border border-surface-200 dark:border-surface-200 bg-surface-0">
      <div className="flex items-center gap-3">
        <div className="w-8 h-8 rounded-lg bg-surface-100 dark:bg-surface-100 flex items-center justify-center">
          <Cpu className="w-4 h-4 text-brand-500" />
        </div>
        <div>
          <p className="text-sm font-medium text-surface-900 dark:text-surface-900">{bid.auctionTitle}</p>
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50">
            Your bid: <span className="font-mono font-semibold">{bid.yourBid} ERG</span>
          </p>
        </div>
      </div>
      <div className="flex items-center gap-3">
        <StatusBadge status={bid.status} />
        <CountdownTimer endsAt={bid.endsAt} />
      </div>
    </div>
  );
}

// ── Auction Detail Panel ───────────────────────────────────────────────────

function AuctionDetailPanel({
  auction,
  onClose,
  onBid,
}: {
  auction: Auction;
  onClose: () => void;
  onBid: (auction: Auction) => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" />
      <div
        className="relative w-full max-w-2xl rounded-2xl bg-surface-0 dark:bg-surface-50 p-6 shadow-xl border border-surface-200 dark:border-surface-200 max-h-[90vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          onClick={onClose}
          className="absolute top-4 right-4 p-1 rounded-lg hover:bg-surface-100 dark:hover:bg-surface-100 transition-colors"
        >
          <X className="w-5 h-5 text-surface-800/60" />
        </button>

        {/* Header */}
        <div className="flex items-start gap-4 mb-5">
          <div className="w-12 h-12 rounded-xl bg-brand-100 dark:bg-brand-900/30 flex items-center justify-center text-brand-700 dark:text-brand-400 text-sm font-bold">
            {auction.providerAvatar}
          </div>
          <div className="flex-1">
            <div className="flex items-center gap-2 mb-1">
              <h2 className="text-xl font-bold text-surface-900 dark:text-surface-900">{auction.gpuType}</h2>
              {auction.isHot && (
                <span className="flex items-center gap-1 bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400 rounded-full px-2 py-0.5 text-xs font-bold">
                  <Flame className="w-3 h-3" /> Hot
                </span>
              )}
            </div>
            <p className="text-sm text-surface-800/60 dark:text-surface-800/50">
              by {auction.provider} · {auction.region}
            </p>
          </div>
          <StatusBadge status={auction.status} />
        </div>

        {/* Description */}
        <p className="text-sm text-surface-800/70 dark:text-surface-800/60 mb-5">{auction.description}</p>

        {/* Specs Grid */}
        <div className="grid grid-cols-4 gap-3 mb-5">
          <div className="bg-surface-100 dark:bg-surface-100 rounded-lg p-3 text-center">
            <p className="text-xs text-surface-800/50 mb-1">VRAM</p>
            <p className="text-lg font-bold text-surface-900 dark:text-surface-900">{auction.vram}GB</p>
          </div>
          <div className="bg-surface-100 dark:bg-surface-100 rounded-lg p-3 text-center">
            <p className="text-xs text-surface-800/50 mb-1">Duration</p>
            <p className="text-lg font-bold text-surface-900 dark:text-surface-900">{auction.duration}</p>
          </div>
          <div className="bg-surface-100 dark:bg-surface-100 rounded-lg p-3 text-center">
            <p className="text-xs text-surface-800/50 mb-1">Type</p>
            <p className="text-sm font-bold text-surface-900 dark:text-surface-900 mt-0.5">
              {auction.auctionType === "gpu_time" ? "GPU Time" : "Serving"}
            </p>
          </div>
          <div className="bg-surface-100 dark:bg-surface-100 rounded-lg p-3 text-center">
            <p className="text-xs text-surface-800/50 mb-1">Bids</p>
            <p className="text-lg font-bold text-surface-900 dark:text-surface-900">{auction.bidCount}</p>
          </div>
        </div>

        {/* Bid History Chart (full size) */}
        <BidHistoryChart bidHistory={auction.bidHistory} />

        {/* Bid History Table */}
        <div className="mt-4">
          <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-900 mb-2">Recent Bids</h3>
          <div className="rounded-lg border border-surface-200 dark:border-surface-200 overflow-hidden">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-surface-100 dark:bg-surface-100">
                  <th className="text-left px-3 py-2 text-xs font-medium text-surface-800/50">Bidder</th>
                  <th className="text-right px-3 py-2 text-xs font-medium text-surface-800/50">Amount</th>
                  <th className="text-right px-3 py-2 text-xs font-medium text-surface-800/50">Time</th>
                </tr>
              </thead>
              <tbody>
                {[...auction.bidHistory].reverse().map((bid, i) => (
                  <tr key={i} className={cn("border-t border-surface-100 dark:border-surface-100", i === 0 && "bg-brand-50/50 dark:bg-brand-900/10")}>
                    <td className="px-3 py-2 font-mono text-xs text-surface-800/70 dark:text-surface-800/60">
                      {bid.bidder}
                    </td>
                    <td className={cn("px-3 py-2 text-right font-mono font-semibold", i === 0 ? "text-brand-600 dark:text-brand-400" : "text-surface-900 dark:text-surface-900")}>
                      {bid.amount} ERG
                    </td>
                    <td className="px-3 py-2 text-right text-xs text-surface-800/50 dark:text-surface-800/50">
                      {formatTimeAgo(bid.timestamp)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        {/* Bottom bar */}
        <div className="flex items-center justify-between mt-5 pt-4 border-t border-surface-200 dark:border-surface-200">
          <div>
            <p className="text-xs text-surface-800/50">Current Bid</p>
            <p className="text-2xl font-bold text-surface-900 dark:text-surface-900">
              {auction.currentBid} <span className="text-sm font-normal text-surface-800/40">ERG</span>
            </p>
          </div>
          <div className="flex items-center gap-3">
            <CountdownTimer endsAt={auction.endsAt} />
            <button
              onClick={() => {
                onClose();
                setTimeout(() => onBid(auction), 100);
              }}
              className="px-6 py-2.5 rounded-lg text-sm font-semibold bg-brand-600 text-white hover:bg-brand-700 active:scale-95 shadow-sm hover:shadow-md transition-all"
            >
              Place Bid
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Utility ────────────────────────────────────────────────────────────────

function formatTimeAgo(date: Date): string {
  const diff = Date.now() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);
  if (days > 0) return `${days}d ago`;
  if (hours > 0) return `${hours}h ago`;
  if (minutes > 0) return `${minutes}m ago`;
  return "just now";
}

// ── Main Page ──────────────────────────────────────────────────────────────

export default function AuctionPage() {
  const [activeTab, setActiveTab] = useState<"browse" | "my_bids">("browse");
  const [auctions, setAuctions] = useState<Auction[]>(MOCK_AUCTIONS);
  const [myBids, setMyBids] = useState<UserBid[]>(MOCK_MY_BIDS);
  const [bidModalAuction, setBidModalAuction] = useState<Auction | null>(null);
  const [detailAuction, setDetailAuction] = useState<Auction | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [filters, setFilters] = useState<AuctionFilters>({
    gpuType: "All",
    minVram: null,
    maxPrice: null,
    region: "All",
    auctionType: "All",
    status: "All",
    sortBy: "ending_soon",
  });

  // ── Filtered & sorted auctions ──
  const filteredAuctions = useMemo(() => {
    let result = auctions.filter((a) => {
      if (filters.gpuType !== "All" && a.gpuType !== filters.gpuType) return false;
      if (filters.region !== "All" && a.region !== filters.region) return false;
      if (filters.auctionType !== "All" && a.auctionType !== filters.auctionType) return false;
      if (filters.status !== "All" && a.status !== filters.status) return false;
      if (filters.minVram !== null && a.vram < filters.minVram) return false;
      if (filters.maxPrice !== null && a.currentBid > filters.maxPrice) return false;
      return true;
    });

    result.sort((a, b) => {
      switch (filters.sortBy) {
        case "ending_soon":
          return a.endsAt.getTime() - b.endsAt.getTime();
        case "price_low":
          return a.currentBid - b.currentBid;
        case "price_high":
          return b.currentBid - a.currentBid;
        case "most_bids":
          return b.bidCount - a.bidCount;
        case "newest":
          return b.id.localeCompare(a.id);
        default:
          return 0;
      }
    });

    return result;
  }, [auctions, filters]);

  // ── Stats ──
  const stats = useMemo(() => {
    const totalVolume = auctions.reduce((sum, a) => sum + a.currentBid, 0);
    const activeCount = auctions.filter((a) => a.status === "active" || a.status === "ending_soon").length;
    const hotCount = auctions.filter((a) => a.isHot).length;
    const avgBid = auctions.length > 0 ? totalVolume / auctions.length : 0;
    return { totalVolume, activeCount, hotCount, avgBid };
  }, [auctions]);

  // ── Handlers ──
  const handlePlaceBid = useCallback((auctionId: string, amount: number) => {
    setAuctions((prev) =>
      prev.map((a) => {
        if (a.id === auctionId) {
          const newBid = {
            bidder: "0xYOU...abcd",
            amount,
            timestamp: new Date(),
          };
          return {
            ...a,
            currentBid: amount,
            minimumBid: parseFloat((amount + 0.5).toFixed(1)),
            bidCount: a.bidCount + 1,
            bidHistory: [...a.bidHistory, newBid],
          };
        }
        return a;
      })
    );

    // Update my bids
    const auction = auctions.find((a) => a.id === auctionId);
    if (auction) {
      setMyBids((prev) => {
        const existing = prev.find((b) => b.auctionId === auctionId);
        if (existing) {
          return prev.map((b) =>
            b.auctionId === auctionId
              ? { ...b, yourBid: amount, currentBid: amount, status: "won" as const }
              : b
          );
        }
        return [
          ...prev,
          {
            auctionId,
            auctionTitle: `${auction.gpuType} ${auction.vram}GB`,
            gpuType: auction.gpuType,
            yourBid: amount,
            currentBid: amount,
            status: "won" as const,
            endsAt: auction.endsAt,
            bidPlacedAt: new Date(),
          },
        ];
      });
    }
  }, [auctions]);

  const handleCreateAuction = useCallback(() => {
    // Placeholder - just show feedback
    setShowCreateModal(false);
  }, []);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center gap-2">
            <Gavel className="w-6 h-6 text-brand-600" />
            <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-900">Compute Auctions</h1>
          </div>
          <button
            onClick={() => setShowCreateModal(true)}
            className={cn(
              "flex items-center gap-1.5 rounded-lg px-4 py-2 text-sm font-semibold transition-all",
              "bg-brand-600 text-white hover:bg-brand-700 active:scale-95",
              "shadow-sm hover:shadow-md"
            )}
          >
            <Plus className="w-4 h-4" />
            Create Auction
          </button>
        </div>
        <p className="text-surface-800/60 dark:text-surface-800/50">
          Bid on GPU time and model serving slots. Pay with ERG, settle on-chain.
        </p>
      </div>

      {/* Stats Bar */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
        <div className="rounded-xl border border-surface-200 dark:border-surface-200 bg-surface-0 p-3">
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50 mb-1">Active Auctions</p>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-900">{stats.activeCount}</p>
        </div>
        <div className="rounded-xl border border-surface-200 dark:border-surface-200 bg-surface-0 p-3">
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50 mb-1">Total Volume</p>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-900">{stats.totalVolume.toFixed(1)} <span className="text-sm font-normal text-surface-800/40">ERG</span></p>
        </div>
        <div className="rounded-xl border border-surface-200 dark:border-surface-200 bg-surface-0 p-3">
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50 mb-1">Avg. Bid</p>
          <p className="text-xl font-bold text-surface-900 dark:text-surface-900">{stats.avgBid.toFixed(1)} <span className="text-sm font-normal text-surface-800/40">ERG</span></p>
        </div>
        <div className="rounded-xl border border-surface-200 dark:border-surface-200 bg-surface-0 p-3">
          <p className="text-xs text-surface-800/50 dark:text-surface-800/50 mb-1">Trending</p>
          <div className="flex items-center gap-1">
            <Flame className="w-4 h-4 text-amber-500" />
            <p className="text-xl font-bold text-surface-900 dark:text-surface-900">{stats.hotCount}</p>
          </div>
        </div>
      </div>

      {/* Tab switcher */}
      <div className="flex gap-1 mb-6 border-b border-surface-200 dark:border-surface-200">
        <button
          onClick={() => setActiveTab("browse")}
          className={cn(
            "px-4 py-2.5 text-sm font-medium border-b-2 transition-colors",
            activeTab === "browse"
              ? "border-brand-600 text-brand-600 dark:text-brand-500"
              : "border-transparent text-surface-800/50 hover:text-surface-800/70"
          )}
        >
          Browse Auctions
        </button>
        <button
          onClick={() => setActiveTab("my_bids")}
          className={cn(
            "px-4 py-2.5 text-sm font-medium border-b-2 transition-colors",
            activeTab === "my_bids"
              ? "border-brand-600 text-brand-600 dark:text-brand-500"
              : "border-transparent text-surface-800/50 hover:text-surface-800/70"
          )}
        >
          My Bids
          {myBids.length > 0 && (
            <span className="ml-1.5 rounded-full bg-brand-100 text-brand-700 dark:bg-brand-900/30 dark:text-brand-400 px-1.5 py-0.5 text-xs">
              {myBids.length}
            </span>
          )}
        </button>
      </div>

      {/* Browse Auctions Tab */}
      {activeTab === "browse" && (
        <>
          {/* Filters */}
          <FiltersBar
            filters={filters}
            onFiltersChange={setFilters}
            totalResults={filteredAuctions.length}
          />

          {/* Auction Grid */}
          {filteredAuctions.length === 0 ? (
            <div className="text-center py-16">
              <Gavel className="w-12 h-12 text-surface-800/20 dark:text-surface-800/20 mx-auto mb-3" />
              <p className="text-sm text-surface-800/50 dark:text-surface-800/50">No auctions match your filters.</p>
              <button
                onClick={() => setFilters({
                  gpuType: "All",
                  minVram: null,
                  maxPrice: null,
                  region: "All",
                  auctionType: "All",
                  status: "All",
                  sortBy: "ending_soon",
                })}
                className="text-sm text-brand-600 dark:text-brand-400 hover:underline mt-2"
              >
                Clear all filters
              </button>
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 animate-fade-in">
              {filteredAuctions.map((auction) => (
                <AuctionCard
                  key={auction.id}
                  auction={auction}
                  onBid={setBidModalAuction}
                  onSelect={setDetailAuction}
                />
              ))}
            </div>
          )}

          {/* Footer */}
          <div className="mt-8 text-sm text-surface-800/50 dark:text-surface-800/50 text-center">
            All prices in ERG. Auctions are settled on the Ergo blockchain.
            <span className="block mt-1 text-surface-800/30 dark:text-surface-800/30">
              Showing mock data — connect to a live relay for real auctions.
            </span>
          </div>
        </>
      )}

      {/* My Bids Tab */}
      {activeTab === "my_bids" && (
        <div className="animate-fade-in">
          <MyBidsSection bids={myBids} />
        </div>
      )}

      {/* Bid Modal */}
      <BidModal
        auction={bidModalAuction}
        isOpen={!!bidModalAuction}
        onClose={() => setBidModalAuction(null)}
        onConfirm={handlePlaceBid}
      />

      {/* Create Auction Modal */}
      <CreateAuctionModal
        isOpen={showCreateModal}
        onClose={() => setShowCreateModal(false)}
        onCreate={handleCreateAuction}
      />

      {/* Detail Panel */}
      {detailAuction && (
        <AuctionDetailPanel
          auction={detailAuction}
          onClose={() => setDetailAuction(null)}
          onBid={setBidModalAuction}
        />
      )}
    </div>
  );
}
