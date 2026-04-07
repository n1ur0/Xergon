import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// PATCH handler — update provider status
// ---------------------------------------------------------------------------

export async function PATCH(
  request: Request,
  { params }: { params: Promise<{ pk: string }> },
) {
  const { pk } = await params;

  try {
    const body = await request.json();
    const { status } = body as { status?: string };

    if (!status || !["active", "suspended", "pending"].includes(status)) {
      return NextResponse.json(
        { success: false, error: "Invalid status. Must be 'active', 'suspended', or 'pending'." },
        { status: 400 },
      );
    }

    if (!pk || pk.length < 10) {
      return NextResponse.json(
        { success: false, error: "Invalid provider public key." },
        { status: 400 },
      );
    }

    // In production, this would update the relay/backend state.
    // For now, we acknowledge the change.
    return NextResponse.json({ success: true });
  } catch {
    return NextResponse.json(
      { success: false, error: "Invalid request body." },
      { status: 400 },
    );
  }
}
