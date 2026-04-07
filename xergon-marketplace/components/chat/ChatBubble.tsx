"use client";

import { useState, useEffect, useCallback } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ChatBubbleProps {
  isOpen: boolean;
  onToggle: () => void;
  unreadCount?: number;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ChatBubble({ isOpen, onToggle, unreadCount = 0 }: ChatBubbleProps) {
  const [isHovered, setIsHovered] = useState(false);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onToggle();
      }
    },
    [isOpen, onToggle]
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div className="fixed bottom-5 right-5 z-50 flex flex-col items-end gap-3">
      {/* Widget container (animated expand/collapse) */}
      <div
        className={`
          transition-all duration-300 ease-in-out origin-bottom-right
          ${isOpen ? "opacity-100 scale-100 translate-y-0" : "opacity-0 scale-90 translate-y-4 pointer-events-none"}
        `}
        aria-hidden={!isOpen}
      />

      {/* Floating action button */}
      <button
        onClick={onToggle}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
        aria-label={isOpen ? "Close chat" : "Open chat"}
        aria-expanded={isOpen}
        className={`
          relative flex items-center justify-center w-14 h-14 rounded-full
          shadow-lg transition-all duration-200 ease-out
          focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500 focus-visible:ring-offset-2
          ${
            isOpen
              ? "bg-surface-900 text-surface-0 hover:bg-surface-800 dark:bg-surface-100 dark:text-surface-900 dark:hover:bg-surface-200"
              : "bg-brand-600 text-white hover:bg-brand-500 hover:shadow-xl"
          }
          ${isHovered ? "scale-105" : "scale-100"}
        `}
      >
        {/* Chat icon (bubble) */}
        <svg
          className="w-6 h-6"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>

        {/* Close icon (X) when open */}
        {isOpen && (
          <svg
            className="w-6 h-6 absolute"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        )}

        {/* Unread indicator dot */}
        {unreadCount > 0 && !isOpen && (
          <span
            className="absolute -top-1 -right-1 flex items-center justify-center min-w-[20px] h-5 px-1
                       rounded-full bg-red-500 text-white text-[10px] font-bold leading-none
                       animate-pulse"
            aria-label={`${unreadCount} unread messages`}
          >
            {unreadCount > 99 ? "99+" : unreadCount}
          </span>
        )}
      </button>
    </div>
  );
}
