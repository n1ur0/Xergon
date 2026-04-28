import { NextResponse } from "next/server";

import { RELAY_BASE } from "@/lib/api/server-sdk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Dispute {
  id: string;
  type: "quality" | "downtime" | "payment" | "fraud";
  status: "open" | "investigating" | "resolved" | "dismissed";
  reporterAddress: string;
  providerPk: string;
  description: string;
  evidence: string[];
  createdAt: string;
  updatedAt: string;
  resolvedAt?: string;
  resolution?: string;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const disputes: Dispute[] = [
  {
    id: "DSP-042",
    type: "quality",
    status: "open",
    reporterAddress: "9h4k7f2a1b3c8d5e6",
    providerPk: "0x" + "a".repeat(64),
    description: "Provider returned garbled/incomplete responses for llama-3.1-70b over the past 2 hours. Multiple users affected.",
    evidence: [
      "Screenshot of garbled response — 2024-03-15 14:22 UTC",
      "User report: 'Model hallucinating badly, returning partial JSON'",
      "Relay logs showing 418 errors from endpoint",
    ],
    createdAt: new Date(Date.now() - 7_200_000).toISOString(),
    updatedAt: new Date(Date.now() - 7_200_000).toISOString(),
  },
  {
    id: "DSP-041",
    type: "downtime",
    status: "investigating",
    reporterAddress: "2j7qw3e8m1n4p6r9s",
    providerPk: "0x" + "b".repeat(64),
    description: "Provider was offline for 6 hours without notice, disrupting active rentals.",
    evidence: [
      "Uptime monitor showing 6h gap — 2024-03-14 08:00–14:00 UTC",
      "3 affected rental IDs: RNT-881, RNT-882, RNT-883",
    ],
    createdAt: new Date(Date.now() - 72_000_000).toISOString(),
    updatedAt: new Date(Date.now() - 24_000_000).toISOString(),
  },
  {
    id: "DSP-040",
    type: "payment",
    status: "resolved",
    reporterAddress: "5k9m2n7p3q8r1t4v6",
    providerPk: "0x" + "c".repeat(64),
    description: "Provider claims withdrawal of 120 ERG was not received. Transaction appears stuck.",
    evidence: [
      "TxID: abc123def456... (pending for 48h)",
      "Provider wallet balance before and after withdrawal",
    ],
    createdAt: new Date(Date.now() - 172_800_000).toISOString(),
    updatedAt: new Date(Date.now() - 96_000_000).toISOString(),
    resolvedAt: new Date(Date.now() - 96_000_000).toISOString(),
    resolution: "Transaction confirmed after node resync. Provider received funds. No action needed.",
  },
  {
    id: "DSP-039",
    type: "fraud",
    status: "dismissed",
    reporterAddress: "8w2e4r6t1y3u5i7o9",
    providerPk: "0x" + "d".repeat(64),
    description: "User claims provider is using a different model than advertised (mistral-7b instead of llama-3.1-70b).",
    evidence: [
      "Side-by-side comparison of outputs showing similar patterns",
      "Latency too low for 70b model",
    ],
    createdAt: new Date(Date.now() - 259_200_000).toISOString(),
    updatedAt: new Date(Date.now() - 200_000_000).toISOString(),
    resolvedAt: new Date(Date.now() - 200_000_000).toISOString(),
    resolution: "Investigation showed provider routes to multiple backends. Model fingerprinting inconclusive. Dismissed due to insufficient evidence.",
  },
  {
    id: "DSP-038",
    type: "quality",
    status: "open",
    reporterAddress: "1a3b5c7d9e2f4g6h8",
    providerPk: "0x" + "e".repeat(64),
    description: "Consistently high latency (>2s) on qwen2.5-72b requests during peak hours.",
    evidence: [
      "Latency logs showing p99 > 2000ms between 18:00–22:00 UTC",
      "Comparison with same model on other providers showing <400ms",
    ],
    createdAt: new Date(Date.now() - 3_600_000).toISOString(),
    updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
  },
];

// ---------------------------------------------------------------------------
// GET handler — list all disputes
// ---------------------------------------------------------------------------

export async function GET() {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/admin/disputes`, {
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (!res.ok) {
      return NextResponse.json({ disputes, degraded: true });
    }

    const data = await res.json();
    const list: Dispute[] = data?.disputes ?? (Array.isArray(data) ? data : []);
    return NextResponse.json({ disputes: list, degraded: false });
  } catch {
    return NextResponse.json({ disputes, degraded: true });
  }
}

// ---------------------------------------------------------------------------
// POST handler — create a new dispute
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { type, providerPk, description, evidence } = body as {
      type?: string;
      providerPk?: string;
      description?: string;
      evidence?: string[];
    };

    if (!type || !["quality", "downtime", "payment", "fraud"].includes(type)) {
      return NextResponse.json(
        { error: "Invalid dispute type." },
        { status: 400 },
      );
    }

    if (!providerPk || providerPk.length < 10) {
      return NextResponse.json(
        { error: "Invalid provider public key." },
        { status: 400 },
      );
    }

    if (!description || description.length < 10) {
      return NextResponse.json(
        { error: "Description must be at least 10 characters." },
        { status: 400 },
      );
    }

    const now = new Date().toISOString();
    const id = `DSP-${String(43 + Math.floor(Math.random() * 1000)).padStart(3, "0")}`;

    const newDispute: Dispute = {
      id,
      type: type as Dispute["type"],
      status: "open",
      reporterAddress: "system",
      providerPk,
      description,
      evidence: evidence ?? [],
      createdAt: now,
      updatedAt: now,
    };

    return NextResponse.json({ id: newDispute.id, status: newDispute.status });
  } catch {
    return NextResponse.json(
      { error: "Invalid request body." },
      { status: 400 },
    );
  }
}
