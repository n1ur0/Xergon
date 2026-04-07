"use client";

import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Message {
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

interface MessageBubbleProps {
  message: Message;
  currentUserId?: string;
  onReply?: (messageId: string) => void;
  onCopy?: (content: string) => void;
  onFlag?: (messageId: string) => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffDays = Math.floor(diffMs / 86_400_000);

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
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

export function MessageBubble({
  message,
  currentUserId,
  onReply,
  onCopy,
  onFlag,
}: MessageBubbleProps) {
  const [showActions, setShowActions] = useState(false);
  const isOwn = message.senderId === currentUserId;

  return (
    <div
      className={`group flex gap-2.5 ${isOwn ? "flex-row-reverse" : "flex-row"}`}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      {/* Avatar */}
      <div className="flex-shrink-0 pt-0.5">
        <div
          className={`relative w-8 h-8 rounded-full ${avatarColor(message.senderName)} flex items-center justify-center text-xs font-semibold text-white`}
          title={message.senderName}
        >
          {initials(message.senderName)}

          {/* Role badge */}
          {message.senderRole === "provider" && (
            <span className="absolute -bottom-0.5 -right-0.5 w-3.5 h-3.5 rounded-full bg-emerald-500 border-2 border-surface-0 dark:border-surface-900" title="Provider" />
          )}
        </div>
      </div>

      {/* Message body */}
      <div className={`max-w-[75%] ${isOwn ? "items-end" : "items-start"} flex flex-col`}>
        {/* Sender name + time */}
        <div className={`flex items-center gap-2 mb-0.5 text-xs ${isOwn ? "flex-row-reverse" : "flex-row"}`}>
          <span className="font-medium text-surface-900 dark:text-surface-100">
            {message.senderName}
          </span>
          {message.senderRole === "provider" && (
            <span className="px-1.5 py-0 rounded text-[10px] font-medium bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300">
              Provider
            </span>
          )}
          <span className="text-surface-800/40" title={new Date(message.timestamp).toLocaleString()}>
            {formatTime(message.timestamp)}
          </span>
          {/* Read status */}
          {isOwn && (
            <span className="text-surface-800/30" title="Read">
              <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="20 6 9 17 4 12" />
              </svg>
            </span>
          )}
        </div>

        {/* Reply reference */}
        {message.replyTo && (
          <div className={`mb-1 px-3 py-1.5 rounded-lg border-l-2 border-brand-400 bg-brand-50/50 dark:bg-brand-900/10 text-xs text-surface-800/60 ${isOwn ? "text-right" : ""}`}>
            <span className="font-medium text-brand-600 dark:text-brand-400">Replying to a message</span>
          </div>
        )}

        {/* Content bubble */}
        <div
          className={`relative rounded-2xl px-4 py-2.5 text-sm leading-relaxed ${
            isOwn
              ? "bg-brand-600 text-white rounded-br-md"
              : "bg-surface-100 text-surface-900 dark:bg-surface-800 dark:text-surface-100 rounded-bl-md"
          } ${message.flagged ? "ring-2 ring-red-400" : ""}`}
        >
          {/* Markdown content */}
          <div className={`prose prose-sm max-w-none ${isOwn ? "prose-invert" : "dark:prose-invert"}`}>
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {message.content}
            </ReactMarkdown>
          </div>

          {/* Action buttons */}
          {showActions && (
            <div
              className={`absolute flex items-center gap-1 ${
                isOwn ? "left-0 -translate-x-full mr-2" : "right-0 translate-x-full ml-2"
              } top-1/2 -translate-y-1/2`}
            >
              <button
                onClick={() => onReply?.(message.id)}
                className="p-1.5 rounded-lg bg-surface-0 dark:bg-surface-700 text-surface-600 dark:text-surface-300 shadow-sm hover:bg-surface-100 dark:hover:bg-surface-600 transition-colors"
                title="Reply"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="9 17 4 12 9 7" />
                  <path d="M20 18v-2a4 4 0 00-4-4H4" />
                </svg>
              </button>
              <button
                onClick={() => onCopy?.(message.content)}
                className="p-1.5 rounded-lg bg-surface-0 dark:bg-surface-700 text-surface-600 dark:text-surface-300 shadow-sm hover:bg-surface-100 dark:hover:bg-surface-600 transition-colors"
                title="Copy"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                  <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
                </svg>
              </button>
              <button
                onClick={() => onFlag?.(message.id)}
                className={`p-1.5 rounded-lg shadow-sm transition-colors ${
                  message.flagged
                    ? "bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400"
                    : "bg-surface-0 dark:bg-surface-700 text-surface-600 dark:text-surface-300 hover:bg-surface-100 dark:hover:bg-surface-600"
                }`}
                title={message.flagged ? "Flagged" : "Flag message"}
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" />
                  <line x1="4" y1="22" x2="4" y2="15" />
                </svg>
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export { formatTime, formatDate };
