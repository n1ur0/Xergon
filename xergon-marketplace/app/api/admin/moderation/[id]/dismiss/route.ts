import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// POST /api/admin/moderation/[id]/dismiss
// ---------------------------------------------------------------------------

export async function POST(
  _request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  if (!id) {
    return NextResponse.json({ error: "ID is required" }, { status: 400 });
  }

  return NextResponse.json({
    success: true,
    id,
    action: "dismissed",
    resolvedAt: new Date().toISOString(),
    resolvedBy: "admin",
  });
}
