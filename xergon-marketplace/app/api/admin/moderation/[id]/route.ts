import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// PATCH /api/admin/moderation/[id] — update moderation status
// ---------------------------------------------------------------------------

export async function PATCH(
  request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  if (!id) {
    return NextResponse.json({ error: "ID is required" }, { status: 400 });
  }

  try {
    const body = await request.json();
    const { status, reason } = body as { status?: string; reason?: string };

    if (!status || !["pending", "approved", "dismissed", "deleted"].includes(status)) {
      return NextResponse.json({ error: "Invalid status. Must be one of: pending, approved, dismissed, deleted" }, { status: 400 });
    }

    return NextResponse.json({
      success: true,
      id,
      status,
      reason: reason ?? undefined,
      resolvedAt: status !== "pending" ? new Date().toISOString() : undefined,
      resolvedBy: status !== "pending" ? "admin" : undefined,
    });
  } catch {
    return NextResponse.json({ error: "Invalid request body" }, { status: 400 });
  }
}
