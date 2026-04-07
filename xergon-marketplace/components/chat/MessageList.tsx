"use client";

import { useState, useMemo } from "react";
import type { Thread } from "./ChatThread";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface MessageListProps {
  threads: Thread[];
  activeThreadId?: string;
  onSelectThread: (threadId: string) => void;
  onCreateThread?: (participantId: string, participantName: string, participantRole: "user" | "provider" | "admin") => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "Just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  return new Date(iso).toLocaleDateString([], { month: "short", day: "numeric" });
}

function initials(name: string): string {
  return name
    .split(/[\s._-]+/)
    .filter(Boolean)
    .map((w) => w[0])
    .join("")
    .toUpperCase()
    .slice(0, 2);
}

function avatarColor(name: string): string {
  const colors = [
    "bg-blue-500", "bg-emerald-500", "bg-violet-500",
    "bg-amber-500", "bg-rose-500", "bg-cyan-500",
    "bg-indigo-500", "bg-teal-500",
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function MessageList({
  threads,
  activeThreadId,
  onSelectThread,
}: MessageListProps) {
  const [search, setSearch] = useState("");
  const [sortOrder, setSortOrder] = useState<"recent" | "unread">("recent");

  const filtered = useMemo(() => {
    let list = [...threads];

    // Search filter
    if (search.trim()) {
      const q = search.toLowerCase();
      list = list.filter(
        (t) =>
          t.participantName.toLowerCase().includes(q) ||
          t.lastMessage.toLowerCase().includes(q),
      );
    }

    // Sort
    if (sortOrder === "recent") {
      list.sort((a, b) => new Date(b.lastMessageAt).getTime() - new Date(a.lastMessageAt).getTime());
    } else {
      // Unread first, then by time
      list.sort((a, b) => {
        if (a.unreadCount > 0 && b.unreadCount === 0) return -1;
        if (a.unreadCount === 0 && b.unreadCount > 0) return 1;
        return new Date(b.lastMessageAt).getTime() - new Date(a.lastMessageAt).getTime();
      });
    }

    return list;
  }, [threads, search, sortOrder]);

  const totalUnread = threads.reduce((sum, t) => sum + t.unreadCount, 0);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-4 py-3 border-b border-surface-200 dark:border-surface-700">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold text-surface-900 dark:text-surface-100">Messages</h2>
            {totalUnread > 0 && (
              <span className="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-full bg-brand-600 text-white text-[10px] font-bold">
                {totalUnread > 99 ? "99+" : totalUnread}
              </span>
            )}
          </div>
        </div>

        {/* Search */}
        <div className="relative">
          <svg
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search conversations..."
            className="w-full pl-9 pr-3 py-2 text-sm rounded-lg border border-surface-200 dark:border-surface-700 bg-surface-50 dark:bg-surface-800 text-surface-900 dark:text-surface-100 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500/50"
          />
        </div>

        {/* Sort toggle */}
        <div className="flex items-center gap-2 mt-2">
          <button
            onClick={() => setSortOrder("recent")}
            className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors ${
              sortOrder === "recent"
                ? "bg-brand-100 text-brand-700 dark:bg-brand-900/30 dark:text-brand-300"
                : "text-surface-800/50 hover:bg-surface-100 dark:hover:bg-surface-800"
            }`}
          >
            Recent
          </button>
          <button
            onClick={() => setSortOrder("unread")}
            className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors ${
              sortOrder === "unread"
                ? "bg-brand-100 text-brand-700 dark:bg-brand-900/30 dark:text-brand-300"
                : "text-surface-800/50 hover:bg-surface-100 dark:hover:bg-surface-800"
            }`}
          >
            Unread
          </button>
        </div>
      </div>

      {/* Thread list */}
      <div className="flex-1 overflow-y-auto">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full px-4 text-center">
            <div className="w-12 h-12 rounded-full bg-surface-100 dark:bg-surface-800 flex items-center justify-center mb-3">
              <svg className="w-6 h-6 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </div>
            <p className="text-sm text-surface-800/40">
              {search ? "No conversations match your search" : "No conversations yet"}
            </p>
          </div>
        ) : (
          filtered.map((thread) => (
            <button
              key={thread.id}
              onClick={() => onSelectThread(thread.id)}
              className={`w-full flex items-center gap-3 px-4 py-3 text-left transition-colors border-b border-surface-100 dark:border-surface-800 ${
                activeThreadId === thread.id
                  ? "bg-brand-50 dark:bg-brand-950/30"
                  : "hover:bg-surface-50 dark:hover:bg-surface-800/50"
              }`}
            >
              {/* Avatar */}
              <div className="relative flex-shrink-0">
                <div
                  className={`w-10 h-10 rounded-full ${avatarColor(thread.participantName)} flex items-center justify-center text-xs font-semibold text-white`}
                >
                  {initials(thread.participantName)}
                </div>
                {thread.participantRole === "provider" && (
                  <span className="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-emerald-500 border-2 border-surface-0 dark:border-surface-900" />
                )}
              </div>

              {/* Content */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between gap-2">
                  <span className={`text-sm font-medium truncate ${thread.unreadCount > 0 ? "text-surface-900 dark:text-surface-100" : "text-surface-800/70 dark:text-surface-300"}`}>
                    {thread.participantName}
                  </span>
                  <span className="text-[11px] text-surface-800/40 flex-shrink-0">
                    {timeAgo(thread.lastMessageAt)}
                  </span>
                </div>
                <div className="flex items-center justify-between gap-2 mt-0.5">
                  <p className={`text-xs truncate ${thread.unreadCount > 0 ? "text-surface-800/70 dark:text-surface-300 font-medium" : "text-surface-800/40"}`}>
                    {thread.lastMessage}
                  </p>
                  {thread.unreadCount > 0 && (
                    <span className="flex-shrink-0 inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded-full bg-brand-600 text-white text-[10px] font-bold">
                      {thread.unreadCount}
                    </span>
                  )}
                </div>
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
