"use client";

import { useState, useEffect, useCallback } from "react";
import { ChatBubble } from "./ChatBubble";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChatWidgetConfig {
  /** Relay API base URL (default: from env NEXT_PUBLIC_API_BASE) */
  relayUrl?: string;
  /** API key / wallet public key for auth */
  apiKey?: string;
  /** Default model to pre-select */
  defaultModel?: string;
  /** Position of the widget (currently only bottom-right supported) */
  position?: "bottom-right" | "bottom-left";
  /** Theme override: "light" | "dark" | "system" */
  theme?: "light" | "dark" | "system";
}

interface ChatWidgetProps {
  config?: ChatWidgetConfig;
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

const STORAGE_KEY = "xergon-chat-open";

function loadOpenState(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function persistOpenState(open: boolean) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, String(open));
  } catch {
    // ignore
  }
}

// ---------------------------------------------------------------------------
// Inner widget (lazy-rendered only when open)
// ---------------------------------------------------------------------------

function ChatPanel({ config }: { config: ChatWidgetConfig }) {
  const { relayUrl, apiKey, defaultModel } = config;

  return (
    <div
      className="w-[380px] h-[520px] max-h-[80vh] rounded-2xl border border-surface-200
                 bg-surface-0 shadow-2xl flex flex-col overflow-hidden
                 dark:border-surface-700 dark:bg-surface-900"
      role="dialog"
      aria-label="Chat widget"
    >
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-3 border-b border-surface-200 dark:border-surface-700 bg-brand-50 dark:bg-brand-950/30">
        <div className="w-8 h-8 rounded-full bg-brand-600 flex items-center justify-center">
          <svg className="w-4 h-4 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
        </div>
        <div>
          <p className="text-sm font-semibold text-surface-900 dark:text-surface-0">Xergon Chat</p>
          <p className="text-xs text-surface-800/50">{defaultModel ?? "AI Assistant"}</p>
        </div>
        <div className="ml-auto flex items-center gap-1">
          <span className="w-2 h-2 rounded-full bg-emerald-500" aria-label="Online" />
          <span className="text-xs text-emerald-600 dark:text-emerald-400">Online</span>
        </div>
      </div>

      {/* Messages area placeholder */}
      <div className="flex-1 flex items-center justify-center p-6 text-center">
        <div className="space-y-3">
          <div className="w-12 h-12 mx-auto rounded-xl bg-brand-100 dark:bg-brand-900/40 flex items-center justify-center">
            <svg className="w-6 h-6 text-brand-600 dark:text-brand-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
              <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
              <line x1="12" y1="22.08" x2="12" y2="12" />
            </svg>
          </div>
          <p className="text-sm text-surface-800/60 dark:text-surface-200/60 max-w-[260px]">
            Start a conversation with the AI model. Your messages are sent to the Xergon relay.
          </p>
          {apiKey && (
            <p className="text-xs text-surface-800/30 dark:text-surface-200/30 font-mono truncate max-w-[260px]">
              Auth: {apiKey.slice(0, 8)}...{apiKey.slice(-4)}
            </p>
          )}
        </div>
      </div>

      {/* Input area placeholder */}
      <div className="border-t border-surface-200 dark:border-surface-700 p-3">
        <div className="flex items-center gap-2">
          <input
            type="text"
            placeholder="Type a message..."
            disabled
            className="flex-1 rounded-lg border border-surface-200 dark:border-surface-600 bg-surface-50 dark:bg-surface-800
                       px-3 py-2 text-sm text-surface-900 dark:text-surface-0 placeholder:text-surface-800/30
                       disabled:opacity-50 disabled:cursor-not-allowed"
            aria-label="Chat message input"
          />
          <button
            disabled
            className="rounded-lg bg-brand-600 text-white px-3 py-2 text-sm font-medium
                       disabled:opacity-50 disabled:cursor-not-allowed hover:bg-brand-500 transition-colors"
            aria-label="Send message"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="22" y1="2" x2="11" y2="13" />
              <polygon points="22 2 15 22 11 13 2 9 22 2" />
            </svg>
          </button>
        </div>
        <p className="text-[10px] text-surface-800/20 mt-1.5 text-center">
          Powered by Xergon Network &middot; {relayUrl ?? "Relay"}
        </p>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main ChatWidgetWrapper
// ---------------------------------------------------------------------------

export function ChatWidgetWrapper({ config = {} }: ChatWidgetProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [mounted, setMounted] = useState(false);

  // Resolve config defaults
  const resolvedConfig: ChatWidgetConfig = {
    relayUrl: config.relayUrl ?? process.env.NEXT_PUBLIC_API_BASE,
    apiKey: config.apiKey,
    defaultModel: config.defaultModel ?? "llama-3.3-70b",
    position: config.position ?? "bottom-right",
    theme: config.theme ?? "system",
  };

  // Hydrate open state from localStorage after mount
  useEffect(() => {
    setMounted(true);
    setIsOpen(loadOpenState());
  }, []);

  const handleToggle = useCallback(() => {
    setIsOpen((prev) => {
      const next = !prev;
      persistOpenState(next);
      return next;
    });
  }, []);

  // Don't render until mounted (avoids hydration mismatch)
  if (!mounted) return null;

  return (
    <>
      {/* Widget panel */}
      <div
        className="fixed bottom-[88px] right-5 z-50 transition-all duration-300 ease-in-out origin-bottom-right"
        style={{
          opacity: isOpen ? 1 : 0,
          transform: isOpen ? "scale(1) translateY(0)" : "scale(0.9) translateY(16px)",
          pointerEvents: isOpen ? "auto" : "none",
        }}
        aria-hidden={!isOpen}
      >
        {isOpen && <ChatPanel config={resolvedConfig} />}
      </div>

      {/* Floating bubble button */}
      <ChatBubble isOpen={isOpen} onToggle={handleToggle} />
    </>
  );
}
