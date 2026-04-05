"use client";

import { useRef, useEffect, useCallback, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.min.css";
import { usePlaygroundStore } from "@/lib/stores/playground";
import { usePlaygroundV2Store } from "@/lib/stores/playground-v2";
import { cn } from "@/lib/utils";
import { TokenCounter } from "@/components/playground/TokenCounter";

export function ResponseArea() {
  const messages = usePlaygroundStore((s) => s.messages);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);
  const containerRef = useRef<HTMLDivElement>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Auto-scroll to bottom when messages change or generating state changes
  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [messages, isGenerating]);

  // Clear copied state after 2s
  useEffect(() => {
    if (!copiedId) return;
    const t = setTimeout(() => setCopiedId(null), 2000);
    return () => clearTimeout(t);
  }, [copiedId]);

  const handleCopy = useCallback(async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
    } catch {
      // fallback
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopiedId(id);
    }
  }, []);

  if (messages.length === 0 && !isGenerating) {
    return (
      <div className="flex-1 flex items-center justify-center text-surface-800/30 text-sm">
        Responses will appear here.
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex-1 overflow-y-auto space-y-4">
      {messages.map((msg) => (
        <div
          key={msg.id}
          className={cn(
            "rounded-lg p-4 text-sm",
            msg.role === "user"
              ? "bg-brand-50 text-surface-900 ml-4 md:ml-12"
              : "bg-surface-100 text-surface-800 mr-4 md:mr-12",
          )}
        >
          {msg.role === "assistant" && (
            <div className="text-xs text-surface-800/40 mb-1 flex items-center justify-between">
              <span>
                {msg.model ?? "model"} &middot;{" "}
                {new Date(msg.timestamp).toLocaleTimeString()}
              </span>
              <div className="flex items-center gap-2">
                <TokenCounter
                  promptTokens={msg.inputTokens}
                  completionTokens={msg.outputTokens}
                  costNanoerg={msg.costNanoerg}
                />
                {msg.content && msg.content.trim() && (
                  <button
                    onClick={() => handleCopy(msg.content, msg.id)}
                    className="rounded px-1.5 py-0.5 text-surface-800/30 hover:bg-surface-200 hover:text-surface-800/60 transition-colors"
                    title="Copy response"
                  >
                    {copiedId === msg.id ? (
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <polyline points="20 6 9 17 4 12" />
                      </svg>
                    ) : (
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
                        <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
                      </svg>
                    )}
                  </button>
                )}
              </div>
            </div>
          )}
          {msg.role === "assistant" ? (
            <div className="prose prose-sm max-w-none prose-headings:text-surface-900 prose-p:text-surface-800 prose-a:text-brand-600 prose-a:no-underline hover:prose-a:underline prose-strong:text-surface-900 prose-li:text-surface-800 prose-table:text-surface-800">
              {msg.content ? (
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeHighlight]}
                  components={{
                    pre: ({ children }) => (
                      <pre className="!m-0 !rounded-lg !bg-surface-950 !p-4 !text-surface-200 overflow-x-auto text-[13px] leading-relaxed">
                        {children}
                      </pre>
                    ),
                    code: ({ className, children, ...props }) => {
                      const isInline = !className;
                      if (isInline) {
                        return (
                          <code
                            className="rounded bg-surface-200 px-1.5 py-0.5 text-[13px] font-mono text-brand-700"
                            {...props}
                          >
                            {children}
                          </code>
                        );
                      }
                      return (
                        <code className={className} {...props}>
                          {children}
                        </code>
                      );
                    },
                    table: ({ children }) => (
                      <div className="overflow-x-auto">
                        <table className="min-w-full">{children}</table>
                      </div>
                    ),
                  }}
                >
                  {msg.content}
                </ReactMarkdown>
              ) : isGenerating ? (
                <ThinkingIndicator />
              ) : null}
            </div>
          ) : (
            <div className="whitespace-pre-wrap">{msg.content}</div>
          )}
        </div>
      ))}
      {/* Generating indicator at bottom when streaming */}
      {isGenerating && messages.length > 0 && messages[messages.length - 1].role === "assistant" && messages[messages.length - 1].content && (
        <div className="flex items-center gap-2 text-sm text-surface-800/50 px-4">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-pulse" />
          Streaming...
        </div>
      )}
    </div>
  );
}

/** Animated thinking dots shown while waiting for first token */
function ThinkingIndicator() {
  return (
    <div className="flex items-center gap-1.5 py-1 text-surface-800/40">
      <span className="inline-block h-1.5 w-1.5 rounded-full bg-surface-800/30 animate-bounce [animation-delay:0ms]" />
      <span className="inline-block h-1.5 w-1.5 rounded-full bg-surface-800/30 animate-bounce [animation-delay:150ms]" />
      <span className="inline-block h-1.5 w-1.5 rounded-full bg-surface-800/30 animate-bounce [animation-delay:300ms]" />
      <span className="ml-1 text-xs">Thinking...</span>
    </div>
  );
}

/** Re-export for use in ModelComparison */
export function MarkdownRenderer({ content }: { content: string }) {
  return (
    <div className="prose prose-sm max-w-none prose-headings:text-surface-900 prose-p:text-surface-800 prose-a:text-brand-600 prose-a:no-underline hover:prose-a:underline prose-strong:text-surface-900 prose-li:text-surface-800 prose-table:text-surface-800">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          pre: ({ children }) => (
            <pre className="!m-0 !rounded-lg !bg-surface-950 !p-4 !text-surface-200 overflow-x-auto text-[13px] leading-relaxed">
              {children}
            </pre>
          ),
          code: ({ className, children, ...props }) => {
            const isInline = !className;
            if (isInline) {
              return (
                <code
                  className="rounded bg-surface-200 px-1.5 py-0.5 text-[13px] font-mono text-brand-700"
                  {...props}
                >
                  {children}
                </code>
              );
            }
            return <code className={className} {...props}>{children}</code>;
          },
          table: ({ children }) => (
            <div className="overflow-x-auto">
              <table className="min-w-full">{children}</table>
            </div>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
