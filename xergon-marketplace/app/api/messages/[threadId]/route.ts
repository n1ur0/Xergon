import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types (shared with parent route)
// ---------------------------------------------------------------------------

interface ChatMessage {
  id: string;
  threadId: string;
  senderId: string;
  senderName: string;
  senderAvatar?: string;
  senderRole: "user" | "provider" | "admin";
  content: string;
  timestamp: string;
  readBy: string[];
  replyTo?: string;
  flagged?: boolean;
}

// ---------------------------------------------------------------------------
// In-memory store reference (in production, use a shared DB layer)
// ---------------------------------------------------------------------------

// We import from the parent route for shared store. In production, this
// would be a database layer. For the in-memory demo, we replicate the mock.
const messagesStore: Record<string, ChatMessage[]> = {
  "thread-1": [
    {
      id: "msg-1a",
      threadId: "thread-1",
      senderId: "provider-alpha",
      senderName: "Alpha GPU Node",
      senderRole: "provider",
      content: "Welcome to Alpha GPU Node! How can I help you today?",
      timestamp: new Date(Date.now() - 864_000_000).toISOString(),
      readBy: ["current-user"],
    },
    {
      id: "msg-1b",
      threadId: "thread-1",
      senderId: "current-user",
      senderName: "You",
      senderRole: "user",
      content: "I'd like to rent an A100 for running llama-3.1-70b. What's your pricing?",
      timestamp: new Date(Date.now() - 600_000).toISOString(),
      readBy: ["current-user"],
    },
    {
      id: "msg-1c",
      threadId: "thread-1",
      senderId: "provider-alpha",
      senderName: "Alpha GPU Node",
      senderRole: "provider",
      content: "Our pricing is **0.0002 ERG/1K tokens** for input and **0.0004 ERG/1K tokens** for output. We offer a 10% discount for rentals over 24 hours.\n\nHere are the specs:\n- GPU: NVIDIA A100 80GB\n- Region: EU-West\n- Uptime: 99.9%\n- Avg latency: ~150ms",
      timestamp: new Date(Date.now() - 300_000).toISOString(),
      readBy: [],
    },
  ],
  "thread-2": [
    {
      id: "msg-2a",
      threadId: "thread-2",
      senderId: "provider-beta",
      senderName: "Beta Compute",
      senderRole: "provider",
      content: "Thanks for choosing Beta Compute!",
      timestamp: new Date(Date.now() - 1_728_000_000).toISOString(),
      readBy: ["current-user"],
    },
    {
      id: "msg-2b",
      threadId: "thread-2",
      senderId: "current-user",
      senderName: "You",
      senderRole: "user",
      content: "Great service, thanks for the quick response time!",
      timestamp: new Date(Date.now() - 3_600_000).toISOString(),
      readBy: ["current-user", "provider-beta"],
    },
    {
      id: "msg-2c",
      threadId: "thread-2",
      senderId: "provider-beta",
      senderName: "Beta Compute",
      senderRole: "provider",
      content: "Thanks for the review! Let me know if you need anything.",
      timestamp: new Date(Date.now() - 3_600_000).toISOString(),
      readBy: ["current-user", "provider-beta"],
    },
  ],
  "thread-3": [
    {
      id: "msg-3a",
      threadId: "thread-3",
      senderId: "provider-gamma",
      senderName: "Gamma Inference",
      senderRole: "provider",
      content: "We're experiencing some downtime. ETA: 30 minutes.",
      timestamp: new Date(Date.now() - 18_000_000).toISOString(),
      readBy: [],
    },
  ],
};

// ---------------------------------------------------------------------------
// GET /api/messages/[threadId] — get thread messages
// ---------------------------------------------------------------------------

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ threadId: string }> },
) {
  const { threadId } = await params;

  if (!threadId) {
    return NextResponse.json({ error: "threadId is required" }, { status: 400 });
  }

  const threadMessages = messagesStore[threadId] ?? [];

  return NextResponse.json({
    threadId,
    messages: threadMessages,
    total: threadMessages.length,
  });
}

// ---------------------------------------------------------------------------
// POST /api/messages/[threadId] — send message to existing thread
// ---------------------------------------------------------------------------

export async function POST(
  request: Request,
  { params }: { params: Promise<{ threadId: string }> },
) {
  const { threadId } = await params;

  try {
    const body = await request.json();
    const { content, replyTo } = body as { content?: string; replyTo?: string };

    if (!content || content.trim().length === 0) {
      return NextResponse.json({ error: "Message content is required" }, { status: 400 });
    }

    const newMessage: ChatMessage = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      threadId,
      senderId: "current-user",
      senderName: "You",
      senderRole: "user",
      content: content.trim(),
      timestamp: new Date().toISOString(),
      readBy: ["current-user"],
      replyTo,
    };

    if (!messagesStore[threadId]) {
      messagesStore[threadId] = [];
    }
    messagesStore[threadId].push(newMessage);

    return NextResponse.json({ message: newMessage });
  } catch {
    return NextResponse.json({ error: "Failed to send message" }, { status: 500 });
  }
}

// ---------------------------------------------------------------------------
// DELETE /api/messages/[threadId] — delete thread
// ---------------------------------------------------------------------------

export async function DELETE(
  _request: Request,
  { params }: { params: Promise<{ threadId: string }> },
) {
  const { threadId } = await params;

  if (!threadId) {
    return NextResponse.json({ error: "threadId is required" }, { status: 400 });
  }

  // In production, delete from DB. Here we just acknowledge.
  delete messagesStore[threadId];

  return NextResponse.json({ success: true, threadId });
}
