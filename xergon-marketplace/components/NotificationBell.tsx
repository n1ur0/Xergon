"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Bell,
  CheckCheck,
  ChevronRight,
  Sparkles,
  CreditCard,
  AlertTriangle,
  Info,
  Tag,
} from "lucide-react";

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

const TYPE_ICON: Record<NotificationType, React.ReactNode> = {
  rental_started: <Sparkles className="h-3.5 w-3.5 text-purple-500" />,
  rental_completed: <Sparkles className="h-3.5 w-3.5 text-purple-500" />,
  rental_expiring: <AlertTriangle className="h-3.5 w-3.5 text-purple-500" />,
  payment_received: <CreditCard className="h-3.5 w-3.5 text-emerald-500" />,
  dispute_update: <AlertTriangle className="h-3.5 w-3.5 text-red-500" />,
  new_model: <Sparkles className="h-3.5 w-3.5 text-cyan-500" />,
  price_change: <Tag className="h-3.5 w-3.5 text-amber-500" />,
  system: <Info className="h-3.5 w-3.5 text-blue-500" />,
};

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function NotificationBell() {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [unreadCount, setUnreadCount] = useState(0);
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [loading, setLoading] = useState(true);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const prevCountRef = useRef(0);
  const [animate, setAnimate] = useState(false);

  // Fetch unread count and latest notifications
  const fetchData = useCallback(async () => {
    try {
      const [countRes, notifsRes] = await Promise.all([
        fetch("/api/user/notifications/unread-count"),
        fetch("/api/user/notifications?limit=5"),
      ]);

      if (countRes.ok) {
        const { count } = await countRes.json();
        setUnreadCount(count);
        if (count > prevCountRef.current && prevCountRef.current > 0) {
          setAnimate(true);
          setTimeout(() => setAnimate(false), 600);
        }
        prevCountRef.current = count;
      }

      if (notifsRes.ok) {
        const data = await notifsRes.json();
        setNotifications(data.notifications);
      }
    } catch {
      // Silently fail
    } finally {
      setLoading(false);
    }
  }, []);

  // Poll every 30s
  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30_000);
    return () => clearInterval(interval);
  }, [fetchData]);

  // Close dropdown on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [open]);

  const handleMarkAllRead = async () => {
    try {
      await fetch("/api/user/notifications", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ all: true }),
      });
      setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
      setUnreadCount(0);
    } catch {
      // Silently fail
    }
  };

  const handleNotifClick = async (notif: Notification) => {
    if (!notif.read) {
      try {
        await fetch("/api/user/notifications", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ id: notif.id }),
        });
        setNotifications((prev) =>
          prev.map((n) => (n.id === notif.id ? { ...n, read: true } : n)),
        );
        setUnreadCount((c) => Math.max(0, c - 1));
      } catch {
        // Silently fail
      }
    }
    setOpen(false);
    if (notif.actionUrl) {
      router.push(notif.actionUrl);
    }
  };

  return (
    <div className="relative" ref={dropdownRef}>
      {/* Bell button */}
      <button
        onClick={() => setOpen((o) => !o)}
        className="relative inline-flex items-center justify-center rounded-lg p-2 text-surface-800/70 hover:text-surface-900 hover:bg-surface-100 transition-colors min-h-[44px] min-w-[44px]"
        aria-label={`Notifications${unreadCount > 0 ? ` (${unreadCount} unread)` : ""}`}
      >
        <Bell className="h-5 w-5" />
        {unreadCount > 0 && (
          <span
            className={`absolute -right-0.5 -top-0.5 flex h-5 min-w-[20px] items-center justify-center rounded-full bg-red-500 px-1 text-[10px] font-bold text-white ${
              animate ? "scale-125 transition-transform" : "transition-transform"
            }`}
          >
            {unreadCount > 99 ? "99+" : unreadCount}
          </span>
        )}
      </button>

      {/* Dropdown */}
      {open && (
        <div className="absolute right-0 top-full mt-2 w-80 rounded-xl border border-surface-200 bg-white shadow-xl dark:border-surface-700 dark:bg-surface-900 z-50">
          {/* Header */}
          <div className="flex items-center justify-between border-b border-surface-200 px-4 py-3 dark:border-surface-700">
            <h3 className="text-sm font-semibold text-surface-900 dark:text-surface-0">
              Notifications
            </h3>
            {unreadCount > 0 && (
              <button
                onClick={handleMarkAllRead}
                className="text-xs text-brand-600 hover:underline"
              >
                Mark all read
              </button>
            )}
          </div>

          {/* List */}
          {loading ? (
            <div className="space-y-2 p-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="animate-pulse h-14 rounded-lg bg-surface-100 dark:bg-surface-800" />
              ))}
            </div>
          ) : notifications.length === 0 ? (
            <div className="py-8 text-center">
              <CheckCheck className="mx-auto h-6 w-6 text-surface-800/20" />
              <p className="mt-2 text-xs text-surface-800/40">
                No notifications yet
              </p>
            </div>
          ) : (
            <div className="max-h-80 overflow-y-auto divide-y divide-surface-100 dark:divide-surface-800">
              {notifications.map((notif) => (
                <button
                  key={notif.id}
                  onClick={() => handleNotifClick(notif)}
                  className={`flex w-full items-start gap-2.5 px-4 py-3 text-left transition-colors hover:bg-surface-50 dark:hover:bg-surface-800 ${
                    !notif.read ? "bg-brand-50/50 dark:bg-brand-900/10" : ""
                  }`}
                >
                  <div className="mt-0.5 shrink-0">
                    {TYPE_ICON[notif.type] || TYPE_ICON.system}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p
                      className={`text-xs ${
                        !notif.read
                          ? "font-semibold text-surface-900 dark:text-surface-0"
                          : "text-surface-800/70 dark:text-surface-300/70"
                      }`}
                    >
                      {notif.title}
                    </p>
                    <p className="mt-0.5 text-xs text-surface-800/50 line-clamp-2">
                      {notif.message}
                    </p>
                    <p className="mt-1 text-[10px] text-surface-800/30">
                      {timeAgo(notif.createdAt)}
                    </p>
                  </div>
                  {!notif.read && (
                    <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-500" />
                  )}
                </button>
              ))}
            </div>
          )}

          {/* Footer */}
          <div className="border-t border-surface-200 px-4 py-2.5 dark:border-surface-700">
            <Link
              href="/profile/notifications"
              onClick={() => setOpen(false)}
              className="flex items-center justify-center gap-1 text-xs font-medium text-brand-600 hover:underline"
            >
              View All
              <ChevronRight className="h-3.5 w-3.5" />
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
