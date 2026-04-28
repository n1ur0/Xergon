import { NextResponse } from "next/server";

import { RELAY_BASE } from "@/lib/api/server-sdk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ProviderAdmin {
  providerPk: string;
  endpoint: string;
  region: string;
  models: string[];
  status: "active" | "suspended" | "pending";
  totalEarningsNanoErg: number;
  totalRequests: number;
  averageLatencyMs: number;
  uptime: number;
  registeredAt: string;
  lastHeartbeat: string;
  slashCount: number;
  disputeCount: number;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

function mockProviders(): ProviderAdmin[] {
  const now = Date.now();
  const regions = ["US", "EU", "Asia", "US", "EU", "Asia", "US", "EU", "US", "Asia", "EU", "US"];
  const statuses: Array<"active" | "suspended" | "pending"> = [
    "active", "active", "active", "active", "active", "active",
    "active", "active", "suspended", "pending", "active", "active",
  ];
  const modelSets = [
    ["llama-3.1-70b", "llama-3.1-8b", "mistral-7b"],
    ["qwen2.5-72b", "qwen2.5-7b"],
    ["deepseek-coder-33b", "codestral-22b"],
    ["llama-3.1-70b", "gemma-2-27b", "phi-3-medium"],
    ["mistral-7b", "yi-1.5-34b", "command-r-35b"],
    ["llama-3.1-8b", "phi-3-medium", "mistral-7b"],
    ["qwen2.5-72b", "llama-3.1-70b", "deepseek-coder-33b"],
    ["gemma-2-27b", "codestral-22b", "mistral-7b"],
    ["llama-3.1-70b", "llama-3.1-8b", "qwen2.5-72b", "mistral-7b"],
    ["phi-3-medium", "yi-1.5-34b"],
    ["deepseek-coder-33b", "llama-3.1-70b", "command-r-35b"],
    ["mistral-7b", "qwen2.5-7b", "gemma-2-27b"],
  ];

  return Array.from({ length: 12 }, (_, i) => ({
    providerPk: `0x${Array.from({ length: 64 }, () =>
      Math.floor(Math.random() * 16).toString(16),
    ).join("")}`,
    endpoint: `https://node-${String(i + 1).padStart(3, "0")}.xergon.${regions[i].toLowerCase()}.net`,
    region: regions[i],
    models: modelSets[i],
    status: statuses[i],
    totalEarningsNanoErg: Math.floor(100_000_000 + Math.random() * 2_000_000_000),
    totalRequests: Math.floor(1_000 + Math.random() * 500_000),
    averageLatencyMs: Math.floor(50 + Math.random() * 400),
    uptime: statuses[i] === "suspended" ? 0 : Math.round((92 + Math.random() * 8) * 10) / 10,
    registeredAt: new Date(now - Math.floor(Math.random() * 30 * 86400000)).toISOString(),
    lastHeartbeat: statuses[i] === "suspended"
      ? new Date(now - 86400000).toISOString()
      : new Date(now - Math.floor(Math.random() * 300000)).toISOString(),
    slashCount: statuses[i] === "suspended" ? 2 : Math.random() > 0.7 ? 1 : 0,
    disputeCount: statuses[i] === "suspended" ? 3 : Math.random() > 0.8 ? 1 : 0,
  }));
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET() {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/admin/providers`, {
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (!res.ok) {
      return NextResponse.json({ providers: mockProviders(), degraded: true });
    }

    const data = await res.json();
    const providers: ProviderAdmin[] = data?.providers ?? (Array.isArray(data) ? data : []);
    return NextResponse.json({ providers, degraded: false });
  } catch {
    return NextResponse.json({ providers: mockProviders(), degraded: true });
  }
}
