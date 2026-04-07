import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// POST /api/messages/[threadId]/read — mark thread as read
// ---------------------------------------------------------------------------

export async function POST(
  _request: Request,
  { params }: { params: Promise<{ threadId: string }> },
) {
  const { threadId } = await params;

  if (!threadId) {
    return NextResponse.json({ error: "threadId is required" }, { status: 400 });
  }

  // In production, update the read status in the database.
  // Here we just acknowledge the operation.
  return NextResponse.json({
    success: true,
    threadId,
    readAt: new Date().toISOString(),
  });
}
