import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface WithdrawRequest {
  amountNanoErg: number;
  destinationAddress: string;
}

interface WithdrawResponse {
  txId: string;
  amountNanoErg: number;
  status: "pending" | "processing";
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = (await request.json()) as WithdrawRequest;

    const { amountNanoErg, destinationAddress } = body;

    // Validate amount
    if (!amountNanoErg || amountNanoErg < 1_000_000) {
      return NextResponse.json(
        { error: "Minimum withdrawal amount is 0.001 ERG" },
        { status: 400 },
      );
    }

    // Validate destination address (basic Ergo address format: starts with 3 or 9)
    if (
      !destinationAddress ||
      !/^3[a-km-zA-HJ-NP-Z1-9]{8,}$/.test(destinationAddress)
    ) {
      return NextResponse.json(
        { error: "Invalid Ergo destination address" },
        { status: 400 },
      );
    }

    // Mock: generate a fake txId
    const txId = Array.from({ length: 64 }, () =>
      "0123456789abcdef"[Math.floor(Math.random() * 16)],
    ).join("");

    const response: WithdrawResponse = {
      txId,
      amountNanoErg,
      status: "pending",
    };

    return NextResponse.json(response);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
