/**
 * POST /api/ergoauth
 *
 * Generates an ErgoAuth challenge for the given Ergo address.
 *
 * Request body: { address: string }
 * Response: ErgoAuthRequest with nonce, signingMessage, sigmaBoolean, replyTo
 *
 * The challenge is stored in memory with a 5-minute TTL. The client
 * presents the challenge to the user's wallet for signing.
 */

import { NextRequest, NextResponse } from "next/server";
import type { ErgoAuthRequest, PendingChallenge } from "@/lib/ergoauth/types";
import {
  generateNonce,
  buildSigningMessage,
  addressToSigmaBoolean,
  CHALLENGE_TTL_MS,
} from "@/lib/ergoauth/challenge";
import { setPendingChallenge } from "@/lib/ergoauth/challenge-store";

// ── POST handler ──────────────────────────────────────────────────────────

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { address } = body as { address?: string };

    // Validate address
    if (!address || typeof address !== "string") {
      return NextResponse.json(
        { error: "Missing required field: address" },
        { status: 400 }
      );
    }

    // Basic Ergo address format check
    // Mainnet P2PK starts with '3', P2SH with '9'
    // Testnet P2PK starts with 'b' or 'B'
    if (!/^[39bB]/.test(address) || address.length < 30) {
      return NextResponse.json(
        { error: "Invalid Ergo address format" },
        { status: 400 }
      );
    }

    // Generate challenge components
    const nonce = generateNonce();
    const signingMessage = buildSigningMessage(nonce, address);
    const sigmaBoolean = addressToSigmaBoolean(address);

    // Build the replyTo URL pointing to our verify endpoint
    const baseUrl = getBaseUrl(request);
    const replyTo = `${baseUrl}/api/ergoauth/verify`;

    // Store the pending challenge
    const challenge: PendingChallenge = {
      nonce,
      address,
      signingMessage,
      sigmaBoolean,
      expiresAt: Date.now() + CHALLENGE_TTL_MS,
    };
    setPendingChallenge(challenge);

    // Build the response
    const ergoAuthRequest: ErgoAuthRequest = {
      address,
      signingMessage,
      sigmaBoolean,
      userMessage: `Sign to authenticate with Xergon`,
      messageSeverity: "INFORMATION",
      replyTo,
    };

    return NextResponse.json({
      ...ergoAuthRequest,
      nonce, // Include nonce so client can poll
    });
  } catch (err) {
    console.error("[ErgoAuth] Error generating challenge:", err);
    return NextResponse.json(
      { error: "Failed to generate challenge" },
      { status: 500 }
    );
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/**
 * Derive the base URL from the request.
 * Uses the Origin or Host header, falling back to localhost.
 */
function getBaseUrl(request: NextRequest): string {
  const forwarded = request.headers.get("x-forwarded-host");
  const proto = request.headers.get("x-forwarded-proto") ?? "https";
  const host = forwarded ?? request.headers.get("host") ?? "localhost:3000";

  if (forwarded) {
    return `${proto}://${host}`;
  }

  // Check for Origin header (sent by browsers)
  const origin = request.headers.get("origin");
  if (origin) {
    return origin;
  }

  return `${proto}://${host}`;
}
