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
// In-memory store (mirrors the notifications route store)
// ---------------------------------------------------------------------------

const notificationStore = new Map<string, Notification[]>();

function getOrCreateNotifications(address: string): Notification[] {
  let notifs = notificationStore.get(address);
  if (!notifs) {
    // Generate lightweight mock data
    let seed = 0;
    for (let i = 0; i < address.length; i++)
      seed = (seed * 31 + address.charCodeAt(i)) | 0;

    function rand(): number {
      seed = (seed * 16807 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    }

    const types: NotificationType[] = [
      "rental_started",
      "rental_completed",
      "rental_expiring",
      "payment_received",
      "dispute_update",
      "new_model",
      "price_change",
      "system",
    ];

    const count = Math.floor(rand() * 10) + 5;
    notifs = Array.from({ length: count }, (_, i) => ({
      id: `notif_${i.toString().padStart(4, "0")}`,
      type: types[Math.floor(rand() * types.length)],
      title: "Notification",
      message: "Mock notification message.",
      read: rand() > 0.4,
      createdAt: new Date(
        Date.now() - Math.floor(rand() * 7 * 86400000),
      ).toISOString(),
    }));
    notificationStore.set(address, notifs);
  }
  return notifs;
}

// ---------------------------------------------------------------------------
// GET handler – returns unread count
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";

    const notifs = getOrCreateNotifications(address);
    const count = notifs.filter((n) => !n.read).length;

    return NextResponse.json({ count });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
