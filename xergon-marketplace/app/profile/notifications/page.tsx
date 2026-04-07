"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Bell,
  CheckCircle2,
  CreditCard,
  AlertTriangle,
  Info,
  Sparkles,
  Tag,
  Settings,
  ChevronLeft,
  CheckCheck,
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

const TYPE_CONFIG: Record<
  NotificationType,
  { icon: React.ReactNode; color: string }
> = {
  rental_started: { icon: <Sparkles className="h-4 w-4" />, color: "text-purple-500 bg-purple-100 dark:bg-purple-900/30" },
  rental_completed: { icon: <CheckCircle2 className="h-4 w-4" />, color: "text-purple-500 bg-purple-100 dark:bg-purple-900/30" },
  rental_expiring: { icon: <AlertTriangle className="h-4 w-4" />, color: "text-purple-500 bg-purple-100 dark:bg-purple-900/30" },
  payment_received: { icon: <CreditCard className="h-4 w-4" />, color: "text-emerald-500 bg-emerald-100 dark:bg-emerald-900/30" },
  dispute_update: { icon: <AlertTriangle className="h-4 w-4" />, color: "text-red-500 bg-red-100 dark:bg-red-900/30" },
  new_model: { icon: <Sparkles className="h-4 w-4" />, color: "text-cyan-500 bg-cyan-100 dark:bg-cyan-900/30" },
  price_change: { icon: <Tag className="h-4 w-4" />, color: "text-amber-500 bg-amber-100 dark:bg-amber-900/30" },
  system: { icon: <Info className="h-4 w-4" />, color: "text-blue-500 bg-blue-100 dark:bg-blue-900/30" },
};

const PAGE_SIZE = 15;

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

export default function NotificationsPage() {
  const router = useRouter();
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [page, setPage] = useState(0);
  const [markingAll, setMarkingAll] = useState(false);

  const fetchNotifications = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams({
        limit: PAGE_SIZE.toString(),
        offset: (page * PAGE_SIZE).toString(),
      });
      const res = await fetch(`/api/user/notifications?${params}`);
      if (res.ok) {
        const data = await res.json();
        setNotifications(data.notifications);
        setTotal(data.total);
      }
    } catch {
      // Silently fail
    } finally {
      setLoading(false);
    }
  }, [page]);

  useEffect(() => {
    fetchNotifications();
  }, [fetchNotifications]);

  const unreadCount = notifications.filter((n) => !n.read).length;

  const handleMarkAllRead = async () => {
    setMarkingAll(true);
    try {
      await fetch("/api/user/notifications", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ all: true }),
      });
      setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
    } catch {
      // Silently fail
    } finally {
      setMarkingAll(false);
    }
  };

  const handleMarkRead = async (id: string) => {
    try {
      await fetch("/api/user/notifications", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id }),
      });
      setNotifications((prev) =>
        prev.map((n) => (n.id === id ? { ...n, read: true } : n)),
      );
    } catch {
      // Silently fail
    }
  };

  const handleClick = async (notif: Notification) => {
    if (!notif.read) {
      await handleMarkRead(notif.id);
    }
    if (notif.actionUrl) {
      router.push(notif.actionUrl);
    }
  };

  const hasMore = notifications.length < total;

  return (
    <div className="mx-auto max-w-3xl px-4 py-8 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 dark:text-surface-0">
            Notifications
          </h1>
          <p className="mt-1 text-sm text-surface-800/50">
            Stay up to date with your Xergon activity.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {unreadCount > 0 && (
            <span className="inline-flex items-center gap-1 rounded-full bg-red-100 px-2.5 py-0.5 text-xs font-medium text-red-700 dark:bg-red-900/30 dark:text-red-300">
              {unreadCount} unread
            </span>
          )}
          <button
            onClick={handleMarkAllRead}
            disabled={markingAll || unreadCount === 0}
            className="inline-flex items-center gap-1.5 rounded-lg border border-surface-300 px-3 py-1.5 text-sm font-medium transition-colors hover:bg-surface-100 disabled:opacity-40 dark:border-surface-600 dark:hover:bg-surface-800"
          >
            <CheckCheck className="h-4 w-4" />
            {markingAll ? "Marking..." : "Mark All Read"}
          </button>
        </div>
      </div>

      {/* Notification List */}
      {loading ? (
        <div className="space-y-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="animate-pulse h-20 rounded-xl bg-surface-200 dark:bg-surface-800" />
          ))}
        </div>
      ) : notifications.length === 0 ? (
        /* Empty state */
        <div className="rounded-2xl border border-dashed border-surface-300 py-20 text-center dark:border-surface-600">
          <CheckCircle2 className="mx-auto h-12 w-12 text-emerald-400" />
          <p className="mt-4 text-lg font-medium text-surface-900 dark:text-surface-0">
            You&apos;re all caught up!
          </p>
          <p className="mt-1 text-sm text-surface-800/50">
            No new notifications at the moment.
          </p>
        </div>
      ) : (
        <div className="divide-y divide-surface-200 rounded-xl border border-surface-200 dark:divide-surface-700 dark:border-surface-700">
          {notifications.map((notif) => {
            const cfg = TYPE_CONFIG[notif.type] || TYPE_CONFIG.system;
            return (
              <button
                key={notif.id}
                onClick={() => handleClick(notif)}
                className={`flex w-full items-start gap-3 p-4 text-left transition-colors hover:bg-surface-50 dark:hover:bg-surface-800 ${
                  !notif.read
                    ? "bg-brand-50/50 dark:bg-brand-900/10"
                    : ""
                }`}
              >
                {/* Icon */}
                <div
                  className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full ${cfg.color}`}
                >
                  {cfg.icon}
                </div>

                {/* Content */}
                <div className="flex-1 min-w-0">
                  <p
                    className={`text-sm ${
                      !notif.read
                        ? "font-semibold text-surface-900 dark:text-surface-0"
                        : "font-medium text-surface-800/70 dark:text-surface-300/70"
                    }`}
                  >
                    {notif.title}
                  </p>
                  <p className="mt-0.5 text-sm text-surface-800/50 line-clamp-2">
                    {notif.message}
                  </p>
                </div>

                {/* Meta */}
                <div className="flex flex-col items-end gap-1 shrink-0">
                  <span className="text-xs text-surface-800/40">
                    {timeAgo(notif.createdAt)}
                  </span>
                  {!notif.read && (
                    <span className="h-2 w-2 rounded-full bg-brand-500" />
                  )}
                </div>
              </button>
            );
          })}
        </div>
      )}

      {/* Load More */}
      {!loading && hasMore && (
        <div className="text-center">
          <button
            onClick={() => setPage((p) => p + 1)}
            className="rounded-lg border border-surface-300 px-6 py-2 text-sm font-medium transition-colors hover:bg-surface-100 dark:border-surface-600 dark:hover:bg-surface-800"
          >
            Load More
          </button>
        </div>
      )}

      {/* Back link */}
      <Link
        href="/profile"
        className="inline-flex items-center gap-1 text-sm text-brand-600 hover:underline"
      >
        <ChevronLeft className="h-4 w-4" />
        Back to Profile
      </Link>
    </div>
  );
}
