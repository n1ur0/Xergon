"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { MessageList } from "@/components/chat/MessageList";
import { ChatThread, type Thread } from "@/components/chat/ChatThread";
import type { Message } from "@/components/chat/MessageBubble";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ThreadData {
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

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function MessagesClient() {
  const [threads, setThreads] = useState<ThreadData[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<string | undefined>();
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);
  const [mobileShowThread, setMobileShowThread] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval>>(undefined);

  // Fetch threads
  const fetchThreads = useCallback(async () => {
    try {
      const res = await fetch("/api/messages");
      if (res.ok) {
        const data = await res.json();
        setThreads(data.threads ?? []);
      }
    } catch {
      // silently fail
    }
  }, []);

  // Fetch messages for active thread
  const fetchMessages = useCallback(async (threadId: string) => {
    try {
      const res = await fetch(`/api/messages/${threadId}`);
      if (res.ok) {
        const data = await res.json();
        setMessages(data.messages ?? []);
      }
    } catch {
      // silently fail
    }
  }, []);

  // Initial load
  useEffect(() => {
    const load = async () => {
      setLoading(true);
      await fetchThreads();
      setLoading(false);
    };
    load();

    // Polling fallback for real-time updates (every 10s)
    pollRef.current = setInterval(fetchThreads, 10_000);

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [fetchThreads]);

  // Fetch messages when thread changes
  useEffect(() => {
    if (activeThreadId) {
      fetchMessages(activeThreadId);
    }
  }, [activeThreadId, fetchMessages]);

  const handleSelectThread = (threadId: string) => {
    setActiveThreadId(threadId);
    setMobileShowThread(true);
  };

  const handleSendMessage = async (threadId: string, content: string, replyTo?: string) => {
    const res = await fetch(`/api/messages/${threadId}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content, replyTo }),
    });

    if (res.ok) {
      const data = await res.json();
      setMessages((prev) => [...prev, data.message]);
      // Refresh threads to update lastMessage
      fetchThreads();
    }
  };

  const handleMarkRead = async (threadId: string) => {
    try {
      await fetch(`/api/messages/${threadId}/read`, { method: "POST" });
      setThreads((prev) =>
        prev.map((t) => (t.id === threadId ? { ...t, unreadCount: 0 } : t)),
      );
    } catch {
      // no-op
    }
  };

  const handleBack = () => {
    setMobileShowThread(false);
  };

  const activeThread = threads.find((t) => t.id === activeThreadId);

  // Convert ThreadData to Thread type
  const threadList: Thread[] = threads.map((t) => ({
    id: t.id,
    participantId: t.participantId,
    participantName: t.participantName,
    participantAvatar: t.participantAvatar,
    participantRole: t.participantRole as "user" | "provider" | "admin",
    lastMessage: t.lastMessage,
    lastMessageAt: t.lastMessageAt,
    unreadCount: t.unreadCount,
    createdAt: t.createdAt,
  }));

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="animate-pulse text-surface-800/40">Loading messages...</div>
      </div>
    );
  }

  return (
    <div className="flex h-full border border-surface-200 dark:border-surface-700 rounded-xl overflow-hidden bg-surface-0 dark:bg-surface-900">
      {/* Thread list sidebar */}
      <div
        className={`w-80 flex-shrink-0 border-r border-surface-200 dark:border-surface-700 bg-surface-0 dark:bg-surface-900 ${
          mobileShowThread ? "hidden md:block" : "block"
        }`}
      >
        <MessageList
          threads={threadList}
          activeThreadId={activeThreadId}
          onSelectThread={handleSelectThread}
        />
      </div>

      {/* Chat thread */}
      <div
        className={`flex-1 ${
          !mobileShowThread ? "hidden md:block" : "block"
        }`}
      >
        {activeThread ? (
          <ChatThread
            thread={activeThread as unknown as Thread}
            messages={messages}
            currentUserId="current-user"
            onSendMessage={handleSendMessage}
            onMarkRead={handleMarkRead}
            onBack={handleBack}
          />
        ) : (
          <div className="flex flex-col items-center justify-center h-full text-center px-4">
            <div className="w-16 h-16 rounded-full bg-surface-100 dark:bg-surface-800 flex items-center justify-center mb-4">
              <svg className="w-8 h-8 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-surface-900 dark:text-surface-100 mb-1">
              Your Messages
            </h3>
            <p className="text-sm text-surface-800/40 max-w-sm">
              Select a conversation or start a new one by visiting a provider's profile.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
