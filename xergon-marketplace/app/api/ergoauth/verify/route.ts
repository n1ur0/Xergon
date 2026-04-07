/**
 * POST /api/ergoauth/verify
 *
 * Verifies an ErgoAuth proof submitted by a wallet.
 *
 * Request body: { proof: string, signedMessage: string, address: string }
 * Response: { success: boolean, address?: string, accessToken?: string, error?: string }
 *
 * The proof is verified against the stored challenge. If valid, a session
 * access token is issued.
 */

import { NextRequest, NextResponse } from "next/server";
import { getPendingChallenge, deletePendingChallenge } from "@/lib/ergoauth/challenge-store";
import { verifySignedMessage } from "@/lib/ergoauth/challenge";

// ── Token generation (simple opaque token) ────────────────────────────────
// In production, use a proper JWT library like jose or jsonwebtoken.

const TOKEN_EXPIRY_MS = 24 * 60 * 60 * 1000; // 24 hours

/**
 * Generate a simple opaque access token.
 * Format: "xergo_<timestamp>_<random>"
 */
function generateAccessToken(address: string): string {
  const timestamp = Date.now().toString(36);
  const random = crypto.getRandomValues(new Uint8Array(16))
    .reduce((acc, b) => acc + b.toString(36).padStart(2, "0"), "");
  return `xergo_${timestamp}_${random}`;
}

// ── POST handler ──────────────────────────────────────────────────────────

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { proof, signedMessage, address, nonce } = body as {
      proof?: string;
      signedMessage?: string;
      address?: string;
      nonce?: string;
    };

    // Validate required fields
    if (!proof || !signedMessage || !address || !nonce) {
      return NextResponse.json(
        {
          success: false,
          error: "Missing required fields: proof, signedMessage, address, nonce",
        },
        { status: 400 }
      );
    }

    // Look up the pending challenge
    const challenge = getPendingChallenge(nonce);
    if (!challenge) {
      return NextResponse.json(
        {
          success: false,
          error: "Invalid or expired challenge. Please request a new one.",
        },
        { status: 410 } // Gone
      );
    }

    // Check if the challenge has expired
    if (challenge.expiresAt < Date.now()) {
      deletePendingChallenge(nonce);
      return NextResponse.json(
        {
          success: false,
          error: "Challenge expired. Please request a new one.",
        },
        { status: 410 }
      );
    }

    // Verify the address matches
    if (challenge.address !== address) {
      return NextResponse.json(
        {
          success: false,
          error: "Address mismatch. The proof must be for the original challenge address.",
        },
        { status: 403 }
      );
    }

    // Verify the signed message matches
    if (challenge.signingMessage !== signedMessage) {
      return NextResponse.json(
        {
          success: false,
          error: "Signed message mismatch.",
        },
        { status: 403 }
      );
    }

    // Verify the proof (stub: accepts structurally valid proofs)
    const isValid = await verifySignedMessage(address, signedMessage, proof);
    if (!isValid) {
      return NextResponse.json(
        {
          success: false,
          error: "Invalid proof. The signature could not be verified.",
        },
        { status: 403 }
      );
    }

    // Challenge is consumed — delete it
    deletePendingChallenge(nonce);

    // Issue access token
    const accessToken = generateAccessToken(address);
    const expiresAt = Date.now() + TOKEN_EXPIRY_MS;

    return NextResponse.json({
      success: true,
      address,
      accessToken,
      expiresAt,
    });
  } catch (err) {
    console.error("[ErgoAuth] Error verifying proof:", err);
    return NextResponse.json(
      {
        success: false,
        error: "Failed to verify proof",
      },
      { status: 500 }
    );
  }
}
