import { NextRequest, NextResponse } from "next/server";

// ── Mock Data ──

const OVERVIEW = {
  score: 90,
  level: "Platinum",
  total_tasks: 4821,
  success_rate: 94,
  rank: 7,
  bonded_erg: 500,
  boost_amount: 5,
};

const BREAKDOWN = [
  { category: "Success Rate", key: "success_rate", value: 94, icon: "🎯" },
  { category: "Latency", key: "latency", value: 87, icon: "⚡" },
  { category: "Uptime", key: "uptime", value: 99, icon: "🟢" },
  { category: "Quality Score", key: "quality", value: 91, icon: "💎" },
  { category: "Community", key: "community", value: 78, icon: "🤝" },
];

const LEADERBOARD = [
  { rank: 1, name: "NeuralForge", score: 98, level: "Diamond", change: 0 },
  { rank: 2, name: "GPUHive", score: 97, level: "Diamond", change: 1 },
  { rank: 3, name: "DeepCompute", score: 96, level: "Diamond", change: -1 },
  { rank: 4, name: "InferX", score: 94, level: "Platinum", change: 2 },
  { rank: 5, name: "TensorNode", score: 93, level: "Platinum", change: 0 },
  { rank: 6, name: "ModelMesh", score: 91, level: "Platinum", change: 1 },
  { rank: 7, name: "Your Node", score: 90, level: "Platinum", change: 3 },
  { rank: 8, name: "ComputeEdge", score: 89, level: "Platinum", change: -1 },
  { rank: 9, name: "AICore", score: 87, level: "Gold", change: 2 },
  { rank: 10, name: "PromptGrid", score: 85, level: "Gold", change: 0 },
  { rank: 11, name: "ZeroLatency", score: 83, level: "Gold", change: -2 },
  { rank: 12, name: "SmartBatch", score: 80, level: "Platinum", change: 1 },
  { rank: 13, name: "DataFlow", score: 78, level: "Gold", change: 1 },
  { rank: 14, name: "RunPodLite", score: 72, level: "Gold", change: 0 },
  { rank: 15, name: "FlashGPU", score: 68, level: "Silver", change: -3 },
];

const HISTORY = {
  labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
  values: [72, 75, 74, 78, 80, 82, 85, 84, 87, 86, 88, 90],
};

// ── Handler ──

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const section = searchParams.get("section");

  switch (section) {
    case "overview":
      return NextResponse.json({ ...OVERVIEW, history: HISTORY });
    case "breakdown":
      return NextResponse.json({ breakdown: BREAKDOWN });
    case "leaderboard":
      return NextResponse.json({ leaderboard: LEADERBOARD });
    default:
      return NextResponse.json({ overview: OVERVIEW, breakdown: BREAKDOWN, leaderboard: LEADERBOARD, history: HISTORY });
  }
}
