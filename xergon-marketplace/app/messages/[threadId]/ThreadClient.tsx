"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import { ChatThread, type Thread } from "@/components/chat/ChatThread";
import type { Message } from "@/components/chat/MessageBubble";

export function ThreadClient() {
  const params = useParams<{ threadId: string }>();
  const threadId = params.threadId;
  const [thread, setThread] = useState<Thread | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchData = useCallback(async () => {
    if (!threadId) return;

    try {
      const [threadsRes, messagesRes] = await Promise.all([
        fetch("/api/messages"),
        fetch(`/api/messages/${threadId}`),
      ]);

      if (threadsRes.ok) {
        const data = await threadsRes.json();
        const found = (data.threads ?? []).find((t: { id: string }) => t.id === threadId);
        if (found) {
          setThread(found as unknown as Thread);
        }
      }

      if (messagesRes.ok) {
        const data = await messagesRes.json();
        setMessages(data.messages ?? []);
      }
    } catch {
      // silently fail
    } finally {
      setLoading(false);
    }
  }, [threadId]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleSendMessage = async (tId: string, content: string, replyTo?: string) => {
    const res = await fetch(`/api/messages/${tId}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content, replyTo }),
    });

    if (res.ok) {
      const data = await res.json();
      setMessages((prev) => [...prev, data.message]);
    }
  };

  const handleMarkRead = async (tId: string) => {
    try {
      await fetch(`/api/messages/${tId}/read`, { method: "POST" });
    } catch {
      // no-op
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-[calc(100vh-4rem)]">
        <div className="animate-pulse text-surface-800/40">Loading conversation...</div>
      </div>
    );
  }

  if (!thread) {
    return (
      <div className="flex flex-col items-center justify-center h-[calc(100vh-4rem)] text-center px-4">
        <div className="w-16 h-16 rounded-full bg-surface-100 dark:bg-surface-800 flex items-center justify-center mb-4">
          <svg className="w-8 h-8 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
        </div>
        <h3 className="text-lg font-semibold text-surface-900 dark:text-surface-100 mb-1">
          Conversation not found
        </h3>
        <p className="text-sm text-surface-800/40">
          This conversation may have been deleted or doesn't exist.
        </p>
        <a
          href="/messages"
          className="mt-4 px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 transition-colors"
        >
          Back to Messages
        </a>
      </div>
    );
  }

  return (
    <div className="h-[calc(100vh-4rem)] border border-surface-200 dark:border-surface-700 rounded-xl overflow-hidden bg-surface-0 dark:bg-surface-900">
      <ChatThread
        thread={thread}
        messages={messages}
        currentUserId="current-user"
        onSendMessage={handleSendMessage}
        onMarkRead={handleMarkRead}
        onBack={() => {
          window.location.href = "/messages";
        }}
      />
    </div>
  );
}
