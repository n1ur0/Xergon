/**
 * Onboarding API routes
 *
 * GET  /api/onboarding/status     — Check if onboarding is completed
 * POST /api/onboarding/complete   — Mark onboarding as complete
 * POST /api/onboarding/save-progress — Save step progress (fire-and-forget)
 */

import { NextRequest, NextResponse } from "next/server";

// In-memory store (in production, use a database keyed by wallet address)
// For now, onboarding state is primarily client-side via localStorage.
// This API exists for future server-side persistence and analytics.

// ── GET /api/onboarding/status ────────────────────────────────────────────

export async function GET() {
  return NextResponse.json({
    status: "ok",
    message: "Onboarding status is managed client-side via localStorage.",
    clientKeys: {
      progress: "xergon_onboarding_progress",
      completed: "xergon_onboarding_completed",
    },
  });
}

// ── POST /api/onboarding/complete ─────────────────────────────────────────

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();

    // Validate minimal required fields
    if (!body || typeof body !== "object") {
      return NextResponse.json(
        { error: "Invalid request body" },
        { status: 400 }
      );
    }

    // In production, persist to database keyed by wallet address
    // For now, log and return success
    console.log("[Onboarding] Completed:", {
      accountType: body.accountType,
      walletConnected: body.wallet?.connected,
      displayName: body.profile?.displayName,
      providerEndpoint: body.provider?.endpointUrl ? "set" : "not set",
      theme: body.preferences?.theme,
      language: body.preferences?.language,
    });

    return NextResponse.json({
      success: true,
      message: "Onboarding completed successfully.",
    });
  } catch (err) {
    console.error("[Onboarding] Error completing:", err);
    return NextResponse.json(
      { error: "Failed to complete onboarding" },
      { status: 500 }
    );
  }
}

// ── PUT /api/onboarding/save-progress ─────────────────────────────────────

export async function PUT(request: NextRequest) {
  try {
    const body = await request.json();

    if (!body || typeof body.currentStep !== "number") {
      return NextResponse.json(
        { error: "Invalid progress data" },
        { status: 400 }
      );
    }

    // In production, persist to database
    console.log("[Onboarding] Progress saved:", {
      step: body.currentStep,
      accountType: body.accountType,
    });

    return NextResponse.json({ success: true });
  } catch (err) {
    console.error("[Onboarding] Error saving progress:", err);
    return NextResponse.json(
      { error: "Failed to save progress" },
      { status: 500 }
    );
  }
}
