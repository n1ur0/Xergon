"use client";

import { useRef, useCallback, useEffect, useState } from "react";
import { usePlaygroundStore } from "@/lib/stores/playground";
import { usePlaygroundV2Store } from "@/lib/stores/playground-v2";
import { useRateLimit, type RateLimitState } from "@/lib/hooks/use-rate-limit";
import { cn } from "@/lib/utils";

interface PromptBoxProps {
  onSubmit?: () => void;
}

function isMac() {
  if (typeof navigator === "undefined") return false;
  return navigator.platform.includes("Mac") || navigator.userAgent.includes("Mac");
}

export function PromptBox({ onSubmit }: PromptBoxProps) {
  const prompt = usePlaygroundStore((s) => s.prompt);
  const setPrompt = usePlaygroundStore((s) => s.setPrompt);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);
  const selectedModel = usePlaygroundStore((s) => s.selectedModel);
  const activeConvoId = usePlaygroundV2Store((s) => s.activeConversationId);
  const conversations = usePlaygroundV2Store((s) => s.conversations);

  // ── Rate limit tracking ──
  const rateLimit = useRateLimit();
  const [rateLimitWarningShown, setRateLimitWarningShown] = useState(false);
  const isRateLimited = rateLimit.isLimited;

  // Show warning toast when rate limit is near (< 10% remaining)
  useEffect(() => {
    if (rateLimit.hasData && rateLimit.percentage !== undefined && rateLimit.percentage < 10 && !rateLimitWarningShown) {
      setRateLimitWarningShown(true);
      // Simple inline warning -- no external toast library needed
      console.warn(`[Xergon] Rate limit warning: ${Math.round(rateLimit.percentage)}% requests remaining`);
    }
    // Reset warning when rate limit resets
    if (rateLimit.percentage === undefined || rateLimit.percentage >= 10) {
      setRateLimitWarningShown(false);
    }
  }, [rateLimit.hasData, rateLimit.percentage, rateLimitWarningShown]);

  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Expose focus method via global for keyboard shortcuts
  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      (window as unknown as Record<string, HTMLTextAreaElement>).__xergon_prompt = el;
    }
    return () => {
      if ((window as unknown as Record<string, HTMLTextAreaElement | undefined>).__xergon_prompt === el) {
        delete (window as unknown as Record<string, HTMLTextAreaElement>).__xergon_prompt;
      }
    };
  }, []);

  // Auto-resize textarea - capped at 50vh on mobile
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    const maxH = typeof window !== "undefined" ? Math.min(window.innerHeight * 0.5, 200) : 200;
    el.style.height = Math.min(el.scrollHeight, maxH) + "px";
  }, [prompt]);

  const activeConvo = activeConvoId ? conversations[activeConvoId] : null;
  const messageCount = activeConvo?.messages.length ?? 0;
  const tokenEstimate = Math.ceil(prompt.length / 4);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Shift+Enter for newline, Enter to send
      if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        onSubmit?.();
        return;
      }
      // Also support Ctrl/Cmd+Enter (backward compat)
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        onSubmit?.();
      }
    },
    [onSubmit],
  );

  return (
    <div className="relative w-full">
      {/* Rate limit indicator */}
      {rateLimit.hasData && (
        <div className="mb-1.5 flex items-center justify-between">
          <div className="flex items-center gap-2 rounded-md px-2 py-1 text-[11px]">
            {rateLimit.requestRemaining !== undefined && rateLimit.requestLimit !== undefined ? (
              <span className={cn(
                "font-medium",
                rateLimit.percentage !== undefined && rateLimit.percentage > 50 ? "text-emerald-600" :
                rateLimit.percentage !== undefined && rateLimit.percentage > 20 ? "text-amber-600" :
                "text-red-600",
              )}>
                {rateLimit.requestRemaining}/{rateLimit.requestLimit} requests
              </span>
            ) : (
              <span className="text-surface-800/40">Unlimited</span>
            )}
            {rateLimit.tokenRemaining !== undefined && rateLimit.tokenLimit !== undefined && (
              <span className="text-surface-800/35 hidden sm:inline">
                {rateLimit.tokenRemaining >= 1000
                  ? `${(rateLimit.tokenRemaining / 1000).toFixed(1)}k/${(rateLimit.tokenLimit / 1000).toFixed(0)}k tokens`
                  : `${rateLimit.tokenRemaining}/${rateLimit.tokenLimit} tokens`}
              </span>
            )}
            {rateLimit.secondsUntilReset > 0 && (
              <span className="text-surface-800/35">
                Resets in {(() => {
                  const s = rateLimit.secondsUntilReset;
                  const m = Math.floor(s / 60);
                  const sec = s % 60;
                  return m > 0 ? `${m}m ${sec}s` : `${sec}s`;
                })()}
              </span>
            )}
          </div>
        </div>
      )}

      {/* Rate limit warning banner */}
      {rateLimit.hasData && rateLimit.percentage !== undefined && rateLimit.percentage < 10 && rateLimit.percentage > 0 && (
        <div className="mb-1.5 flex items-center gap-1.5 rounded-md bg-amber-50 border border-amber-200 px-2.5 py-1.5 text-[11px] text-amber-700">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
            <path d="M12 9v4" />
            <path d="M12 17h.01" />
          </svg>
          <span>Rate limit almost reached ({Math.round(rateLimit.percentage)}% remaining). Consider waiting for reset.</span>
        </div>
      )}

      {/* Rate limited banner */}
      {isRateLimited && (
        <div className="mb-1.5 flex items-center gap-1.5 rounded-md bg-red-50 border border-red-200 px-2.5 py-1.5 text-[11px] text-red-700">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" x2="12" y1="8" y2="12" />
            <line x1="12" x2="12.01" y1="16" y2="16" />
          </svg>
          <span>Rate limit reached. Please wait for the window to reset.</span>
        </div>
      )}

      {/* Model indicator */}
      {selectedModel && (
        <div className="absolute left-3 top-3 flex items-center gap-1 text-[10px] font-medium text-surface-800/30 pointer-events-none select-none">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
            <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
            <line x1="12" x2="12" y1="19" y2="22" />
          </svg>
          <span className="max-w-[120px] truncate">{selectedModel}</span>
        </div>
      )}

      {/* Context indicator */}
      {messageCount > 0 && (
        <div className="absolute right-3 top-3 text-[10px] text-surface-800/25 pointer-events-none select-none">
          {messageCount} msg{messageCount !== 1 ? "s" : ""}
        </div>
      )}

      <textarea
        ref={textareaRef}
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={
          messageCount > 0
            ? "Continue the conversation..."
            : "Ask anything..."
        }
        rows={3}
        disabled={isGenerating}
        className={cn(
          "w-full resize-none rounded-xl border border-surface-200 bg-surface-0 p-3 pt-8 text-sm",
          "md:p-4 md:pt-9",
          "placeholder:text-surface-800/30",
          "focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500",
          "disabled:opacity-50 transition-opacity",
          // Mobile: full-width, no rounded bottom for inline send button
          "max-h-[50vh]",
        )}
        style={{
          paddingBottom: "3.5rem",
        }}
      />

      {/* Bottom bar: token estimate + send button + keyboard hint */}
      <div className="absolute bottom-1.5 left-2 right-2 md:left-3 md:right-3 flex items-center justify-between pointer-events-none select-none">
        <span className="text-[10px] text-surface-800/25 pl-1">
          {tokenEstimate > 0 && `~${tokenEstimate} tokens`}
        </span>
        {/* Send button - always visible, pointer-events enabled */}
        <div className="relative group">
          <button
            type="button"
            onClick={onSubmit}
            disabled={isGenerating || !prompt.trim() || isRateLimited}
            className={cn(
              "pointer-events-auto flex items-center justify-center rounded-lg min-h-[36px] min-w-[36px] md:min-h-[28px] md:min-w-auto md:px-3 md:py-1",
              "text-xs font-medium transition-all",
              prompt.trim() && !isGenerating && !isRateLimited
                ? "bg-brand-600 text-white hover:bg-brand-700 shadow-sm"
                : "bg-surface-100 text-surface-800/30",
            )}
            aria-label="Send message"
            title={isRateLimited ? "Rate limit reached. Wait for reset." : undefined}
          >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 12h14" />
            <path d="m12 5 7 7-7 7" />
          </svg>
          <span className="hidden md:inline ml-1">Send</span>
          </button>
        </div>
      </div>

      {/* Desktop-only keyboard hint */}
      <div className="hidden md:block absolute bottom-2 right-3 text-[10px] text-surface-800/25 pointer-events-none select-none">
        <span className="mr-16">{isMac() ? "Cmd+Enter" : "Ctrl+Enter"}</span>
      </div>

      {/* Loading animation overlay */}
      {isGenerating && (
        <div className="absolute inset-0 flex items-center justify-center rounded-xl bg-surface-0/60 backdrop-blur-[1px]">
          <div className="flex items-center gap-1">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:0ms]" />
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:150ms]" />
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:300ms]" />
          </div>
        </div>
      )}
    </div>
  );
}
