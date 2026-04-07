import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// POST /api/admin/moderation/[id]/approve
// ---------------------------------------------------------------------------

// In-memory reference (shared via import in production; duplicated for route isolation)
const flaggedContent: Array<{ id: string; status: string; resolvedAt?: string; resolvedBy?: string }> = [];

// NOTE: In production, all routes share a DB. For this demo, each sub-route
// operates independently. The ReviewModerationPanel component calls the
// parent /api/admin/moderation endpoint for all operations.

export async function POST(
  _request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  if (!id) {
    return NextResponse.json({ error: "ID is required" }, { status: 400 });
  }

  // Acknowledge approval (production: update DB)
  return NextResponse.json({
    success: true,
    id,
    action: "approved",
    resolvedAt: new Date().toISOString(),
    resolvedBy: "admin",
  });
}
