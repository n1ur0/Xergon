/**
 * POST /api/ergopay/signed/:requestId
 *
 * Receives signed transaction data from the mobile wallet callback.
 * The wallet POSTs back after signing the ErgoPay request.
 */

import { NextRequest, NextResponse } from "next/server";
import { getStoredRequest, updateStoredRequest } from "@/lib/ergopay/store";
import { decodeErgoPayCallback } from "@/lib/ergopay/uri";
import type { ErgoPayTransactionSent } from "@/lib/ergopay/types";

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ requestId: string }> }
) {
  const { requestId } = await params;
  const stored = getStoredRequest(requestId);

  if (!stored) {
    return NextResponse.json(
      { error: "Request not found or expired" },
      { status: 404 }
    );
  }

  if (stored.status === "expired") {
    return NextResponse.json(
      { error: "Request expired" },
      { status: 410 }
    );
  }

  if (stored.status !== "pending") {
    return NextResponse.json(
      { error: `Request already ${stored.status}` },
      { status: 409 }
    );
  }

  try {
    const body = await request.json();
    const result = decodeErgoPayCallback(body);

    if (!result) {
      // Check if wallet sent an error
      if (body && typeof body === "object" && "error" in body) {
        return NextResponse.json(
          { error: "Wallet rejected the transaction", details: body.error },
          { status: 400 }
        );
      }
      return NextResponse.json(
        { error: "Invalid callback payload from wallet" },
        { status: 400 }
      );
    }

    // Store the signed result
    const signedTx =
      typeof body === "object" && "signedTx" in body
        ? (body as { signedTx: string }).signedTx
        : undefined;

    updateStoredRequest(requestId, {
      status: "signed",
      txId: result.txId,
      signedTx: signedTx ?? result.txId,
    });

    const response: ErgoPayTransactionSent = { txId: result.txId };
    return NextResponse.json(response);
  } catch (err) {
    console.error("[ErgoPay] Error processing signed tx:", err);
    return NextResponse.json(
      { error: "Failed to process signed transaction" },
      { status: 500 }
    );
  }
}
