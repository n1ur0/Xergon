import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// POST handler — resolve a dispute
// ---------------------------------------------------------------------------

export async function POST(
  request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  try {
    const body = await request.json();
    const { resolution, action } = body as {
      resolution?: string;
      action?: string;
    };

    if (!id) {
      return NextResponse.json(
        { success: false, error: "Dispute ID is required." },
        { status: 400 },
      );
    }

    if (!action || !["dismiss", "warn_provider", "slash_provider", "suspend_provider"].includes(action)) {
      return NextResponse.json(
        { success: false, error: "Invalid action. Must be 'dismiss', 'warn_provider', 'slash_provider', or 'suspend_provider'." },
        { status: 400 },
      );
    }

    if (!resolution || resolution.length < 5) {
      return NextResponse.json(
        { success: false, error: "Resolution must be at least 5 characters." },
        { status: 400 },
      );
    }

    // In production, this would update the dispute record in the backend.
    return NextResponse.json({ success: true, disputeId: id });
  } catch {
    return NextResponse.json(
      { success: false, error: "Invalid request body." },
      { status: 400 },
    );
  }
}
