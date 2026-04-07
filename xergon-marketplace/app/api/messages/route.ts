import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChatThread {
  id: string;
  participantId: string;
  participantName: string;
  participantAvatar?: string;
  participantRole: "user" | "provider" | "admin";
  lastMessage: string;
  lastMessageAt: string;
  unreadCount: number;
  createdAt: string;
}

export interface ChatMessage {
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
// In-memory store (replace with DB in production)
// ---------------------------------------------------------------------------

const threads: ChatThread[] = [
  {
    id: "thread-1",
    participantId: "provider-alpha",
    participantName: "Alpha GPU Node",
    participantRole: "provider",
    lastMessage: "Your rental on llama-3.1-70b has been activated.",
    lastMessageAt: new Date(Date.now() - 300_000).toISOString(),
    unreadCount: 2,
    createdAt: new Date(Date.now() - 864_000_000).toISOString(),
  },
  {
    id: "thread-2",
    participantId: "provider-beta",
    participantName: "Beta Compute",
    participantRole: "provider",
    lastMessage: "Thanks for the review! Let me know if you need anything.",
    lastMessageAt: new Date(Date.now() - 3_600_000).toISOString(),
    unreadCount: 0,
    createdAt: new Date(Date.now() - 1_728_000_000).toISOString(),
  },
  {
    id: "thread-3",
    participantId: "provider-gamma",
    participantName: "Gamma Inference",
    participantRole: "provider",
    lastMessage: "We're experiencing some downtime. ETA: 30 minutes.",
    lastMessageAt: new Date(Date.now() - 18_000_000).toISOString(),
    unreadCount: 1,
    createdAt: new Date(Date.now() - 2_592_000_000).toISOString(),
  },
];

const messages: Record<string, ChatMessage[]> = {
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
// GET /api/messages — list threads
// ---------------------------------------------------------------------------

export async function GET() {
  try {
    const sortedThreads = [...threads].sort(
      (a, b) => new Date(b.lastMessageAt).getTime() - new Date(a.lastMessageAt).getTime(),
    );

    return NextResponse.json({
      threads: sortedThreads,
      total: sortedThreads.length,
    });
  } catch {
    return NextResponse.json({ error: "Failed to fetch threads" }, { status: 500 });
  }
}

// ---------------------------------------------------------------------------
// POST /api/messages — send message (creates thread if needed)
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { threadId, content, participantId, participantName, participantRole, replyTo } = body as {
      threadId?: string;
      content?: string;
      participantId?: string;
      participantName?: string;
      participantRole?: "user" | "provider" | "admin";
      replyTo?: string;
    };

    if (!content || content.trim().length === 0) {
      return NextResponse.json({ error: "Message content is required" }, { status: 400 });
    }

    let targetThreadId = threadId;

    // Create new thread if needed
    if (!targetThreadId && participantId && participantName) {
      targetThreadId = `thread-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      const newThread: ChatThread = {
        id: targetThreadId,
        participantId,
        participantName,
        participantRole: participantRole ?? "provider",
        lastMessage: content.trim().slice(0, 100),
        lastMessageAt: new Date().toISOString(),
        unreadCount: 0,
        createdAt: new Date().toISOString(),
      };
      threads.unshift(newThread);
      messages[targetThreadId] = [];
    }

    if (!targetThreadId) {
      return NextResponse.json({ error: "threadId or participant info is required" }, { status: 400 });
    }

    // Create message
    const newMessage: ChatMessage = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      threadId: targetThreadId,
      senderId: "current-user",
      senderName: "You",
      senderRole: "user",
      content: content.trim(),
      timestamp: new Date().toISOString(),
      readBy: ["current-user"],
      replyTo,
    };

    if (!messages[targetThreadId]) {
      messages[targetThreadId] = [];
    }
    messages[targetThreadId].push(newMessage);

    // Update thread
    const thread = threads.find((t) => t.id === targetThreadId);
    if (thread) {
      thread.lastMessage = content.trim().slice(0, 100);
      thread.lastMessageAt = new Date().toISOString();
    }

    return NextResponse.json({
      message: newMessage,
      threadId: targetThreadId,
    });
  } catch {
    return NextResponse.json({ error: "Failed to send message" }, { status: 500 });
  }
}
