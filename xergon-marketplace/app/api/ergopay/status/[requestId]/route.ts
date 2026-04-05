/**
 * GET /api/ergopay/status/:requestId
 *
 * Returns the current status of an ErgoPay request.
 * The client polls this endpoint to detect when the wallet has signed.
 */

import { NextRequest, NextResponse } from "next/server";
import { getStoredRequest } from "@/lib/ergopay/store";
import type { ErgoPayStatusResponse } from "@/lib/ergopay/types";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ requestId: string }> }
) {
  const { requestId } = await params;
  const stored = getStoredRequest(requestId);

  if (!stored) {
    return NextResponse.json(
      { error: "Request not found" },
      { status: 404 }
    );
  }

  const response: ErgoPayStatusResponse = {
    requestId: stored.id,
    status: stored.status,
    txId: stored.txId,
    signedTx: stored.signedTx,
  };

  return NextResponse.json(response);
}
