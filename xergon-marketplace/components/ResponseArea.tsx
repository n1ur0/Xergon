"use client";

import { useRef, useEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.min.css";
import { usePlaygroundStore } from "@/lib/stores/playground";

export function ResponseArea() {
  const messages = usePlaygroundStore((s) => s.messages);
  const isGenerating = usePlaygroundStore((s) => s.isGenerating);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when messages change or generating state changes
  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [messages, isGenerating]);

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
          className={`rounded-lg p-4 text-sm ${
            msg.role === "user"
              ? "bg-brand-50 text-surface-900 ml-4 md:ml-12"
              : "bg-surface-100 text-surface-800 mr-4 md:mr-12"
          }`}
        >
          {msg.role === "assistant" && (
            <div className="text-xs text-surface-800/40 mb-1">
              {msg.model ?? "model"} · {new Date(msg.timestamp).toLocaleTimeString()}
            </div>
          )}
          {msg.role === "assistant" ? (
            <div className="prose prose-sm max-w-none prose-headings:text-surface-900 prose-p:text-surface-800 prose-a:text-brand-600 prose-a:no-underline hover:prose-a:underline prose-strong:text-surface-900 prose-li:text-surface-800 prose-table:text-surface-800">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeHighlight]}
                components={{
                  pre: ({ children }) => (
                    <pre className="!m-0 !rounded-lg !bg-gray-900 !p-4 !text-gray-100 overflow-x-auto text-[13px] leading-relaxed">
                      {children}
                    </pre>
                  ),
                  code: ({ className, children, ...props }) => {
                    const isInline = !className;
                    if (isInline) {
                      return (
                        <code
                          className="rounded bg-gray-100 px-1.5 py-0.5 text-[13px] font-mono text-brand-700"
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
            </div>
          ) : (
            <div className="whitespace-pre-wrap">{msg.content}</div>
          )}
        </div>
      ))}
      {isGenerating && (
        <div className="flex items-center gap-2 text-sm text-surface-800/50">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-pulse" />
          Generating...
        </div>
      )}
    </div>
  );
}
