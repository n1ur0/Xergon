import { NextRequest, NextResponse } from "next/server";

// ── Mock Data ──

const OVERVIEW = {
  total_staked: 4250,
  total_earned: 28.74,
  current_apy: 11.2,
  active_bonds: 3,
};

const BONDS = [
  { id: "b1", provider: "NeuralForge", amount: 2000, apy: 12.5, duration: 180, remaining: 142, earned: 47.3, status: "active" },
  { id: "b2", provider: "GPUHive", amount: 1000, apy: 10.2, duration: 90, remaining: 34, earned: 12.8, status: "active" },
  { id: "b3", provider: "InferX", amount: 500, apy: 8.7, duration: 30, remaining: 0, earned: 3.6, status: "completed" },
  { id: "b4", provider: "TensorNode", amount: 750, apy: 11.0, duration: 365, remaining: 290, earned: 22.5, status: "active" },
];

const REWARDS = [
  { id: "r1", amount: 5.23, source: "NeuralForge", timestamp: "2 hours ago", type: "staking" },
  { id: "r2", amount: 2.14, source: "Performance bonus", timestamp: "6 hours ago", type: "performance" },
  { id: "r3", amount: 1.50, source: "GPUHive", timestamp: "1 day ago", type: "staking" },
  { id: "r4", amount: 10.00, source: "Referral: 0x3f2a...", timestamp: "2 days ago", type: "referral" },
  { id: "r5", amount: 3.67, source: "TensorNode", timestamp: "3 days ago", type: "staking" },
  { id: "r6", amount: 0.85, source: "Performance bonus", timestamp: "4 days ago", type: "performance" },
  { id: "r7", amount: 3.60, source: "InferX (completed)", timestamp: "5 days ago", type: "staking" },
  { id: "r8", amount: 2.30, source: "NeuralForge", timestamp: "1 week ago", type: "staking" },
];

const TIERS = [
  { name: "Bronze", min_stake: 100, apy: "6-8%", benefits: ["Basic staking rewards", "Standard APY", "Community support"] },
  { name: "Silver", min_stake: 500, apy: "8-12%", benefits: ["Enhanced APY", "Priority queue access", "Early feature access"] },
  { name: "Gold", min_stake: 2000, apy: "12-18%", benefits: ["Premium APY rates", "Reputation boost", "Dedicated support", "Governance voting"] },
  { name: "Diamond", min_stake: 10000, apy: "18-25%", benefits: ["Maximum APY", "Exclusive provider access", "Multiplied reputation", "VIP support", "Protocol revenue share"] },
];

// ── GET Handler ──

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const section = searchParams.get("section");

  switch (section) {
    case "overview":
      return NextResponse.json(OVERVIEW);
    case "bonds":
      return NextResponse.json({ bonds: BONDS });
    case "rewards":
      return NextResponse.json({ rewards: REWARDS });
    case "tiers":
      return NextResponse.json({ tiers: TIERS });
    default:
      return NextResponse.json({ overview: OVERVIEW, bonds: BONDS, rewards: REWARDS, tiers: TIERS });
  }
}

// ── POST Handler ──

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { amount, duration, provider } = body;

    if (!amount || amount <= 0) {
      return NextResponse.json({ error: "Invalid amount" }, { status: 400 });
    }
    if (!duration || ![30, 90, 180, 365].includes(duration)) {
      return NextResponse.json({ error: "Invalid duration. Must be 30, 90, 180, or 365 days." }, { status: 400 });
    }

    const apyBase = duration === 30 ? 8 : duration === 90 ? 10 : duration === 180 ? 14 : 20;
    const estimatedReturn = ((amount * apyBase) / 100) * (duration / 365);

    // Mock successful bond creation
    return NextResponse.json({
      success: true,
      bond: {
        id: `b${Date.now()}`,
        provider: provider || "NeuralForge",
        amount,
        apy: apyBase,
        duration,
        remaining: duration,
        earned: 0,
        status: "active",
        estimated_return: parseFloat(estimatedReturn.toFixed(2)),
      },
    });
  } catch {
    return NextResponse.json({ error: "Invalid request body" }, { status: 400 });
  }
}
