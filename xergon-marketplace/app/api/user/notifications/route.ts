import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type NotificationType =
  | "rental_started"
  | "rental_completed"
  | "rental_expiring"
  | "payment_received"
  | "dispute_update"
  | "new_model"
  | "price_change"
  | "system";

interface Notification {
  id: string;
  type: NotificationType;
  title: string;
  message: string;
  read: boolean;
  actionUrl?: string;
  createdAt: string;
}

// ---------------------------------------------------------------------------
// Deterministic mock data generator
// ---------------------------------------------------------------------------

function generateMockNotifications(address: string): Notification[] {
  let seed = 0;
  for (let i = 0; i < address.length; i++)
    seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function rand(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  const templates: Array<{
    type: NotificationType;
    title: string;
    message: string;
    actionUrl?: string;
  }> = [
    {
      type: "rental_started",
      title: "Rental Started",
      message: "Your rental for llama-3.1-70b has started.",
      actionUrl: "/profile/rentals",
    },
    {
      type: "rental_completed",
      title: "Rental Completed",
      message: "Your rental for qwen2.5-72b has completed. 2.4M tokens used.",
      actionUrl: "/profile/rentals",
    },
    {
      type: "rental_expiring",
      title: "Rental Expiring Soon",
      message: "Your rental for mistral-7b expires in 30 minutes.",
      actionUrl: "/profile/rentals",
    },
    {
      type: "payment_received",
      title: "Payment Received",
      message: "Received 12.5 ERG for provider services.",
      actionUrl: "/earnings",
    },
    {
      type: "dispute_update",
      title: "Dispute Resolved",
      message: "Dispute #D-0042 has been resolved in your favor.",
      actionUrl: "/profile/rentals",
    },
    {
      type: "new_model",
      title: "New Model Available",
      message: "deepseek-r1-70b is now available on Xergon.",
      actionUrl: "/models",
    },
    {
      type: "price_change",
      title: "Price Change",
      message: "llama-3.1-70b pricing updated: -15% in Europe.",
      actionUrl: "/pricing",
    },
    {
      type: "system",
      title: "System Maintenance",
      message: "Scheduled maintenance on Apr 10, 02:00 UTC. Expect brief downtime.",
    },
  ];

  const count = Math.floor(rand() * 20) + 10;
  const now = Date.now();

  return Array.from({ length: count }, (_, i) => {
    const template = templates[Math.floor(rand() * templates.length)];
    return {
      id: `notif_${i.toString().padStart(4, "0")}`,
      type: template.type,
      title: template.title,
      message: template.message,
      read: rand() > 0.4,
      actionUrl: template.actionUrl,
      createdAt: new Date(now - Math.floor(rand() * 7 * 86400000)).toISOString(),
    };
  }).sort(
    (a, b) =>
      new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime(),
  );
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

const notificationStore = new Map<string, Notification[]>();

function getOrCreateNotifications(address: string): Notification[] {
  let notifs = notificationStore.get(address);
  if (!notifs) {
    notifs = generateMockNotifications(address);
    notificationStore.set(address, notifs);
  }
  return notifs;
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";
    const limit = parseInt(searchParams.get("limit") || "20", 10);
    const offset = parseInt(searchParams.get("offset") || "0", 10);

    const notifs = getOrCreateNotifications(address);
    const sliced = notifs.slice(offset, offset + limit);

    return NextResponse.json({
      notifications: sliced,
      total: notifs.length,
      unreadCount: notifs.filter((n) => !n.read).length,
    });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}

// ---------------------------------------------------------------------------
// PATCH handler – mark notifications as read
// ---------------------------------------------------------------------------

export async function PATCH(request: Request) {
  try {
    const body = await request.json();
    const { address, ids, all } = body;

    const addr =
      address || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";
    const notifs = getOrCreateNotifications(addr);

    let markedCount = 0;

    if (all) {
      for (const n of notifs) {
        if (!n.read) {
          n.read = true;
          markedCount++;
        }
      }
    } else if (Array.isArray(ids)) {
      const idSet = new Set(ids);
      for (const n of notifs) {
        if (idSet.has(n.id) && !n.read) {
          n.read = true;
          markedCount++;
        }
      }
    }

    return NextResponse.json({ success: true, markedCount });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}

// ---------------------------------------------------------------------------
// POST handler – mark single notification as read
// ---------------------------------------------------------------------------

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { address, id } = body;

    if (!id) {
      return NextResponse.json(
        { error: "id is required" },
        { status: 400 },
      );
    }

    const addr =
      address || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";
    const notifs = getOrCreateNotifications(addr);

    const notif = notifs.find((n) => n.id === id);
    if (notif && !notif.read) {
      notif.read = true;
    }

    return NextResponse.json({ success: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
