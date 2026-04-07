"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { MessageBubble, type Message } from "./MessageBubble";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Thread {
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

interface ChatThreadProps {
  thread: Thread;
  messages: Message[];
  currentUserId?: string;
  onSendMessage: (threadId: string, content: string, replyTo?: string) => Promise<void>;
  onMarkRead: (threadId: string) => void;
  onBack?: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ChatThread({
  thread,
  messages: initialMessages,
  currentUserId,
  onSendMessage,
  onMarkRead,
  onBack,
}: ChatThreadProps) {
  const [messages, setMessages] = useState<Message[]>(initialMessages);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [replyTo, setReplyTo] = useState<string | undefined>();
  const [showTyping, setShowTyping] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Mark thread as read
  useEffect(() => {
    onMarkRead(thread.id);
  }, [thread.id, onMarkRead]);

  // Simulate typing indicator when user sends
  const simulateTyping = useCallback(() => {
    setShowTyping(true);
    const timer = setTimeout(() => setShowTyping(false), 2000 + Math.random() * 2000);
    return () => clearTimeout(timer);
  }, []);

  const handleSend = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed || sending) return;

    const newMessage: Message = {
      id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      threadId: thread.id,
      senderId: currentUserId ?? "current-user",
      senderName: "You",
      senderRole: "user",
      content: trimmed,
      timestamp: new Date().toISOString(),
      readBy: [currentUserId ?? "current-user"],
      replyTo,
    };

    setMessages((prev) => [...prev, newMessage]);
    setInput("");
    setReplyTo(undefined);
    setSending(true);

    const clearTyping = simulateTyping();

    try {
      await onSendMessage(thread.id, trimmed, replyTo);
    } catch {
      // Message already added optimistically; keep it
    } finally {
      setSending(false);
      clearTyping();
    }
  }, [input, sending, thread.id, currentUserId, replyTo, onSendMessage, simulateTyping]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleReply = (messageId: string) => {
    setReplyTo(messageId);
    inputRef.current?.focus();
  };

  const handleCopy = (content: string) => {
    navigator.clipboard.writeText(content).catch(() => {
      // fallback: no-op
    });
  };

  // Date grouping
  const getDateSeparator = (msg: Message, prev?: Message) => {
    const msgDate = new Date(msg.timestamp).toDateString();
    const prevDate = prev ? new Date(prev.timestamp).toDateString() : null;
    if (prevDate !== msgDate) {
      return msgDate === new Date().toDateString()
        ? "Today"
        : msgDate === new Date(Date.now() - 86_400_000).toDateString()
          ? "Yesterday"
          : new Date(msg.timestamp).toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
    }
    return null;
  };

  return (
    <div className="flex flex-col h-full">
      {/* Thread header */}
      <div className="flex items-center gap-3 px-4 py-3 border-b border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900">
        {onBack && (
          <button
            onClick={onBack}
            className="p-1.5 rounded-lg hover:bg-surface-100 dark:hover:bg-surface-800 transition-colors text-surface-600 dark:text-surface-300"
            aria-label="Back to threads"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </button>
        )}
        <div className="w-9 h-9 rounded-full bg-brand-100 dark:bg-brand-900/30 flex items-center justify-center text-sm font-semibold text-brand-700 dark:text-brand-300">
          {thread.participantName.split(/[\s._-]+/).filter(Boolean).map((w) => w[0]).join("").toUpperCase().slice(0, 2)}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-semibold text-sm text-surface-900 dark:text-surface-100 truncate">
              {thread.participantName}
            </span>
            {thread.participantRole === "provider" && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300">
                Provider
              </span>
            )}
          </div>
          <p className="text-xs text-surface-800/40">
            {messages.length > 0 ? `${messages.length} messages` : "No messages yet"}
          </p>
        </div>
      </div>

      {/* Messages area */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-4">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="w-16 h-16 rounded-full bg-surface-100 dark:bg-surface-800 flex items-center justify-center mb-4">
              <svg className="w-8 h-8 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </div>
            <h3 className="text-sm font-medium text-surface-900 dark:text-surface-100 mb-1">
              Start a conversation
            </h3>
            <p className="text-xs text-surface-800/40 max-w-xs">
              Send a message to {thread.participantName} to discuss rentals, models, or support.
            </p>
          </div>
        )}

        {messages.map((msg, i) => {
          const separator = getDateSeparator(msg, messages[i - 1]);
          return (
            <div key={msg.id}>
              {separator && (
                <div className="flex items-center gap-3 my-3">
                  <div className="flex-1 h-px bg-surface-200 dark:bg-surface-700" />
                  <span className="text-xs text-surface-800/40 font-medium">{separator}</span>
                  <div className="flex-1 h-px bg-surface-200 dark:bg-surface-700" />
                </div>
              )}
              <MessageBubble
                message={msg}
                currentUserId={currentUserId}
                onReply={handleReply}
                onCopy={handleCopy}
                onFlag={(id) => {
                  setMessages((prev) =>
                    prev.map((m) => (m.id === id ? { ...m, flagged: !m.flagged } : m)),
                  );
                }}
              />
            </div>
          );
        })}

        {/* Typing indicator */}
        {showTyping && (
          <div className="flex gap-2.5">
            <div className="w-8 h-8 rounded-full bg-surface-200 dark:bg-surface-700 flex items-center justify-center text-xs text-surface-500">
              {thread.participantName.split(/[\s._-]+/).filter(Boolean).map((w) => w[0]).join("").toUpperCase().slice(0, 2)}
            </div>
            <div className="bg-surface-100 dark:bg-surface-800 rounded-2xl rounded-bl-md px-4 py-3">
              <div className="flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-surface-400 animate-bounce" style={{ animationDelay: "0ms" }} />
                <span className="w-2 h-2 rounded-full bg-surface-400 animate-bounce" style={{ animationDelay: "150ms" }} />
                <span className="w-2 h-2 rounded-full bg-surface-400 animate-bounce" style={{ animationDelay: "300ms" }} />
              </div>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Reply indicator */}
      {replyTo && (
        <div className="mx-4 mt-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-brand-50 dark:bg-brand-900/10 border border-brand-200 dark:border-brand-800">
          <div className="flex-1 text-xs text-brand-700 dark:text-brand-300">
            Replying to a message
          </div>
          <button
            onClick={() => setReplyTo(undefined)}
            className="p-0.5 rounded hover:bg-brand-100 dark:hover:bg-brand-900/20 text-brand-600 dark:text-brand-400"
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      )}

      {/* Message input */}
      <div className="px-4 py-3 border-t border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900">
        <div className="flex items-end gap-2">
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={`Message ${thread.participantName}...`}
            rows={1}
            className="flex-1 resize-none rounded-xl border border-surface-200 dark:border-surface-700 bg-surface-50 dark:bg-surface-800 px-4 py-2.5 text-sm text-surface-900 dark:text-surface-100 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500/50 max-h-32 overflow-y-auto"
            style={{ minHeight: "42px" }}
          />
          <button
            onClick={handleSend}
            disabled={!input.trim() || sending}
            className="flex-shrink-0 w-10 h-10 rounded-xl bg-brand-600 text-white flex items-center justify-center hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            aria-label="Send message"
          >
            {sending ? (
              <svg className="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="10" strokeDasharray="60" strokeDashoffset="20" />
              </svg>
            ) : (
              <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <line x1="22" y1="2" x2="11" y2="13" />
                <polygon points="22 2 15 22 11 13 2 9 22 2" />
              </svg>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
