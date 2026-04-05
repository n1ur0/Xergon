"use client";

import { useState, useMemo, useCallback, useRef, useEffect } from "react";
import { usePlaygroundV2Store } from "@/lib/stores/playground-v2";
import { cn } from "@/lib/utils";

interface ConversationListProps {
  onNewChat: () => void;
}

export function ConversationList({ onNewChat }: ConversationListProps) {
  const conversations = usePlaygroundV2Store((s) => s.conversations);
  const activeId = usePlaygroundV2Store((s) => s.activeConversationId);
  const setActive = usePlaygroundV2Store((s) => s.setActiveConversation);
  const deleteConvo = usePlaygroundV2Store((s) => s.deleteConversation);
  const clearHistory = usePlaygroundV2Store((s) => s.clearHistory);

  const [search, setSearch] = useState("");
  const [isOpen, setIsOpen] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus search when sidebar opens on mobile
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  const sortedConversations = useMemo(() => {
    const list = Object.values(conversations);
    if (!search.trim()) {
      return list.sort((a, b) => b.updatedAt - a.updatedAt);
    }
    const q = search.toLowerCase();
    return list
      .filter(
        (c) =>
          c.title.toLowerCase().includes(q) ||
          c.model.toLowerCase().includes(q),
      )
      .sort((a, b) => b.updatedAt - a.updatedAt);
  }, [conversations, search]);

  const handleDelete = useCallback(
    (e: React.MouseEvent, id: string) => {
      e.stopPropagation();
      deleteConvo(id);
    },
    [deleteConvo],
  );

  const handleClearHistory = useCallback(() => {
    if (confirmClear) {
      clearHistory();
      setConfirmClear(false);
    } else {
      setConfirmClear(true);
      setTimeout(() => setConfirmClear(false), 3000);
    }
  }, [confirmClear, clearHistory]);

  const formatDate = (ts: number) => {
    const d = new Date(ts);
    const now = new Date();
    const isToday =
      d.getDate() === now.getDate() &&
      d.getMonth() === now.getMonth() &&
      d.getFullYear() === now.getFullYear();
    if (isToday) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    return d.toLocaleDateString([], { month: "short", day: "numeric" });
  };

  const formatTokens = (n: number) => {
    if (n < 1000) return `${n}`;
    return `${(n / 1000).toFixed(1)}k`;
  };

  return (
    <>
      {/* Mobile hamburger toggle */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center justify-center rounded-lg p-2 text-surface-800/50 hover:bg-surface-100 hover:text-surface-800/80 transition-colors"
        aria-label="Toggle conversation list"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <line x1="4" x2="20" y1="12" y2="12" />
          <line x1="4" x2="20" y1="6" y2="6" />
          <line x1="4" x2="20" y1="18" y2="18" />
        </svg>
      </button>

      {/* Mobile overlay */}
      {isOpen && (
        <div
          className="fixed inset-0 z-30 bg-black/30 md:hidden"
          onClick={() => setIsOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={cn(
          "fixed top-0 left-0 z-40 h-full w-72 bg-surface-0 border-r border-surface-200 flex flex-col transition-transform duration-200",
          "md:relative md:z-auto md:translate-x-0",
          isOpen ? "translate-x-0" : "-translate-x-full",
        )}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-3 border-b border-surface-200">
          <span className="text-sm font-semibold text-surface-900">
            History
          </span>
          <div className="flex items-center gap-1">
            <button
              onClick={onNewChat}
              className="flex items-center gap-1 rounded-lg px-2 py-1 text-xs font-medium text-brand-600 hover:bg-brand-50 transition-colors"
              title="New Chat (Ctrl+N)"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 5v14" />
                <path d="M5 12h14" />
              </svg>
              New
            </button>
            <button
              onClick={() => setIsOpen(false)}
              className="flex items-center justify-center rounded-lg p-1 text-surface-800/40 hover:bg-surface-100 hover:text-surface-800/70 transition-colors md:hidden"
              aria-label="Close sidebar"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M18 6 6 18" />
                <path d="m6 6 12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Search */}
        <div className="px-3 py-2">
          <input
            ref={inputRef}
            type="text"
            placeholder="Search conversations..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-1.5 text-xs placeholder:text-surface-800/30 focus:outline-none focus:ring-1 focus:ring-brand-500/40 focus:border-brand-500"
          />
        </div>

        {/* Conversation list */}
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {sortedConversations.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 text-surface-800/30 text-xs">
              <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="mb-2 opacity-40">
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
              <span>{search ? "No matches" : "No conversations yet"}</span>
            </div>
          ) : (
            sortedConversations.map((convo) => (
              <button
                key={convo.id}
                onClick={() => {
                  setActive(convo.id);
                  setIsOpen(false);
                }}
                className={cn(
                  "group w-full text-left rounded-lg px-3 py-2 mb-1 transition-colors",
                  activeId === convo.id
                    ? "bg-brand-50 text-brand-700 border border-brand-200"
                    : "hover:bg-surface-50 text-surface-800 border border-transparent",
                )}
              >
                <div className="flex items-start justify-between gap-1">
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-medium truncate">
                      {convo.title}
                    </div>
                    <div className="mt-0.5 flex items-center gap-2 text-[10px] text-surface-800/40">
                      <span className="truncate max-w-[100px]">
                        {convo.model}
                      </span>
                      <span>{formatDate(convo.updatedAt)}</span>
                      {convo.totalTokens > 0 && (
                        <span className="font-mono">
                          {formatTokens(convo.totalTokens)} tok
                        </span>
                      )}
                    </div>
                  </div>
                  <button
                    onClick={(e) => handleDelete(e, convo.id)}
                    className="flex-shrink-0 rounded p-0.5 text-surface-800/20 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-red-50 transition-all"
                    aria-label="Delete conversation"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M18 6 6 18" />
                      <path d="m6 6 12 12" />
                    </svg>
                  </button>
                </div>
              </button>
            ))
          )}
        </div>

        {/* Footer */}
        {Object.keys(conversations).length > 0 && (
          <div className="border-t border-surface-200 px-3 py-2">
            <button
              onClick={handleClearHistory}
              className={cn(
                "w-full rounded-lg px-3 py-1.5 text-xs font-medium transition-colors",
                confirmClear
                  ? "bg-red-50 text-red-600 hover:bg-red-100"
                  : "text-surface-800/40 hover:bg-surface-50 hover:text-surface-800/70",
              )}
            >
              {confirmClear ? "Click again to confirm" : "Clear all history"}
            </button>
          </div>
        )}
      </aside>
    </>
  );
}
