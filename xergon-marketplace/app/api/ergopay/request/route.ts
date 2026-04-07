/**
 * POST /api/ergopay/request
 *
 * Initiates an ErgoPay transaction. Accepts an ErgoPayRequest, creates
 * a mock unsigned transaction, stores it in memory, and returns the
 * signing request + QR code payload.
 *
 * In production, real tx building would use ergo-lib (WASM) or
 * AppkitPro to construct proper ReducedTransactions.
 */

import { NextRequest, NextResponse } from "next/server";
import type {
  ErgoPayRequest,
  ErgoPaySigningRequest,
  ErgoPayRequestResponse,
  StoredErgoPayRequest,
} from "@/lib/ergopay/types";
import { generateQrPayload } from "@/lib/ergopay/uri";
import { setStoredRequest } from "@/lib/ergopay/store";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getBaseUrl(request: NextRequest): string {
  const forwarded = request.headers.get("x-forwarded-host");
  const proto = request.headers.get("x-forwarded-proto") ?? "https";
  const host = forwarded ?? request.headers.get("host") ?? "localhost:3000";
  if (forwarded) return `${proto}://${host}`;
  const origin = request.headers.get("origin");
  if (origin) return origin;
  return `${proto}://${host}`;
}

function generateId(): string {
  return `ep_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Create a mock unsigned transaction.
 * In production, this would use ergo-lib to:
 *   1. Fetch UTXOs for senderAddress from the Ergo node
 *   2. Select inputs covering amountNanoerg + fee
 *   3. Build proper outputs with change
 *   4. Serialize as ReducedTransaction (base16 CBOR)
 */
function buildMockSigningRequest(req: ErgoPayRequest): ErgoPaySigningRequest {
  const fee = 1_000_000; // 0.001 ERG
  const totalOutput = req.amountNanoerg + fee;

  // Mock reduced tx - in production this is a CBOR-encoded ReducedTransaction
  const mockTx = [
    {
      value: req.amountNanoerg,
      ergoTree: "0008cd03" + Buffer.from(req.recipientAddress).toString("hex").slice(0, 60),
    },
    {
      value: 500_000_000,
      ergoTree: "0008cd03" + Buffer.from(req.senderAddress).toString("hex").slice(0, 60),
    },
  ];

  const unsignedTx = Buffer.from(JSON.stringify(mockTx)).toString("hex");

  return {
    unsignedTx,
    fee,
    inputsTotal: totalOutput + 500_000_000,
    outputsTotal: totalOutput,
    dataInputs: [],
    sendTo: [
      {
        address: req.recipientAddress,
        amount: (req.amountNanoerg / 1e9).toFixed(9) + " ERG",
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

export async function POST(request: NextRequest) {
  try {
    const body = (await request.json()) as ErgoPayRequest;
    const { senderAddress, amountNanoerg, recipientAddress } = body;

    // Validate required fields
    if (!senderAddress || typeof senderAddress !== "string") {
      return NextResponse.json(
        { error: "Missing or invalid senderAddress" },
        { status: 400 }
      );
    }
    if (!recipientAddress || typeof recipientAddress !== "string") {
      return NextResponse.json(
        { error: "Missing or invalid recipientAddress" },
        { status: 400 }
      );
    }
    if (typeof amountNanoerg !== "number" || amountNanoerg <= 0) {
      return NextResponse.json(
        { error: "Missing or invalid amountNanoerg (must be positive number)" },
        { status: 400 }
      );
    }

    // Basic address format check
    if (!/^[39bB]/.test(senderAddress) || senderAddress.length < 30) {
      return NextResponse.json(
        { error: "Invalid sender Ergo address format" },
        { status: 400 }
      );
    }
    if (!/^[39bB]/.test(recipientAddress) || recipientAddress.length < 30) {
      return NextResponse.json(
        { error: "Invalid recipient Ergo address format" },
        { status: 400 }
      );
    }

    // Build signing request
    const signingRequest = buildMockSigningRequest(body);
    const requestId = generateId();
    const baseUrl = getBaseUrl(request);
    const replyTo = `${baseUrl}/api/ergopay/signed/${requestId}`;

    // Store request
    const stored: StoredErgoPayRequest = {
      id: requestId,
      request: body,
      signingRequest,
      replyTo,
      status: "pending",
      createdAt: Date.now(),
      expiresAt: Date.now() + 10 * 60 * 1000,
    };
    setStoredRequest(stored);

    // Generate QR payload
    const qrData = generateQrPayload(signingRequest, baseUrl, requestId);

    const response: ErgoPayRequestResponse = {
      requestId,
      signingRequest,
      qrData,
    };

    return NextResponse.json(response);
  } catch (err) {
    console.error("[ErgoPay] Error creating request:", err);
    return NextResponse.json(
      { error: "Failed to create ErgoPay request" },
      { status: 500 }
    );
  }
}

/**
 * GET /api/ergopay/request?requestId=<id>
 *
 * Returns the ErgoPaySigningRequest for a given request ID.
 * This is the endpoint that mobile wallets fetch when scanning the QR code.
 */
export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const requestId = searchParams.get("requestId");
  if (!requestId) {
    return NextResponse.json(
      { error: "Missing requestId query parameter" },
      { status: 400 }
    );
  }

  const { getStoredRequest } = await import("@/lib/ergopay/store");
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
      { error: "Request already processed" },
      { status: 409 }
    );
  }

  // Return the signing request with replyTo
  return NextResponse.json({
    ...stored.signingRequest,
    replyTo: stored.replyTo,
  });
}
