import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WithdrawalRecord {
  id: string;
  amountNanoErg: number;
  destinationAddress: string;
  txId: string;
  status: "pending" | "completed" | "failed";
  createdAt: string;
  completedAt?: string;
}

// ---------------------------------------------------------------------------
// Mock data generator
// ---------------------------------------------------------------------------

function generateMockHistory(address: string): WithdrawalRecord[] {
  const now = Date.now();
  const statuses: Array<"pending" | "completed" | "failed"> = [
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "pending",
    "completed",
    "failed",
    "completed",
  ];

  let seed = 0;
  for (let i = 0; i < address.length; i++)
    seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function rand(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  return Array.from({ length: 10 }, (_, i) => {
    const status = statuses[i % statuses.length];
    const hoursAgo = i * 48 + Math.floor(rand() * 36);
    const createdAt = new Date(now - hoursAgo * 3600_000).toISOString();
    const completedAt =
      status !== "pending"
        ? new Date(now - (hoursAgo - 2) * 3600_000).toISOString()
        : undefined;
    const amountNanoErg = Math.floor(
      (5_000_000 + rand() * 95_000_000) * 100,
    );

    return {
      id: `wd-${i}-${address.slice(0, 8)}`,
      amountNanoErg,
      destinationAddress: `3${Array.from({ length: 9 }, () =>
        "abcdefghijkmnpqrstuvwxyz123456789"[Math.floor(rand() * 33)],
      ).join("")}`,
      txId: Array.from({ length: 64 }, () =>
        "0123456789abcdef"[Math.floor(rand() * 16)],
      ).join(""),
      status,
      createdAt,
      completedAt,
    };
  });
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";

    const history = generateMockHistory(address);

    return NextResponse.json(history);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
