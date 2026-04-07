"use client";

import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.min.css";
import { cn } from "@/lib/utils";
import { TokenCounter } from "@/components/playground/TokenCounter";

interface StreamingMessageProps {
  content: string;
  isGenerating: boolean;
  model?: string;
  timestamp?: number;
  promptTokens?: number;
  completionTokens?: number;
  costNanoerg?: number;
  onStop?: () => void;
  className?: string;
}

export function StreamingMessage({
  content,
  isGenerating,
  model,
  timestamp,
  promptTokens,
  completionTokens,
  costNanoerg,
  onStop,
  className,
}: StreamingMessageProps) {
  const [copiedBlock, setCopiedBlock] = useState<string | null>(null);
  const [copiedAll, setCopiedAll] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom while streaming
  useEffect(() => {
    if (containerRef.current && isGenerating && content) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [content, isGenerating]);

  // Clear copied state after 2s
  useEffect(() => {
    if (!copiedBlock && !copiedAll) return;
    const t = setTimeout(() => {
      setCopiedBlock(null);
      setCopiedAll(false);
    }, 2000);
    return () => clearTimeout(t);
  }, [copiedBlock, copiedAll]);

  const handleCopyBlock = useCallback(async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedBlock(id);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopiedBlock(id);
    }
  }, []);

  const handleCopyAll = useCallback(async () => {
    if (!content.trim()) return;
    try {
      await navigator.clipboard.writeText(content);
      setCopiedAll(true);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = content;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopiedAll(true);
    }
  }, [content]);

  // Generate a unique code block ID
  const codeBlockId = useMemo(() => {
    let counter = 0;
    return () => `cb-${counter++}`;
  }, []);

  // Check if content likely has code blocks
  const hasCodeBlocks = content.includes("```");

  return (
    <div
      ref={containerRef}
      className={cn("rounded-lg bg-surface-100 text-surface-800 overflow-hidden", className)}
    >
      {/* Header bar */}
      <div className="flex items-center justify-between px-4 py-1.5 border-b border-surface-200/60">
        <div className="flex items-center gap-2 text-xs text-surface-800/40">
          <span className="font-medium">{model ?? "assistant"}</span>
          {timestamp && (
            <span>{new Date(timestamp).toLocaleTimeString()}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <TokenCounter
            promptTokens={promptTokens}
            completionTokens={completionTokens}
            costNanoerg={costNanoerg}
          />
          {/* Stop generating button */}
          {isGenerating && onStop && (
            <button
              onClick={onStop}
              className="flex items-center gap-1 rounded-md bg-red-50 px-2 py-0.5 text-[11px] font-medium text-red-600 hover:bg-red-100 transition-colors"
              title="Stop generating"
            >
              <StopIcon />
              Stop
            </button>
          )}
          {/* Copy all button */}
          {content.trim() && !isGenerating && (
            <button
              onClick={handleCopyAll}
              className="rounded-md px-1.5 py-0.5 text-surface-800/30 hover:bg-surface-200 hover:text-surface-800/60 transition-colors"
              title="Copy response"
            >
              {copiedAll ? <CheckIcon /> : <CopyIcon />}
            </button>
          )}
        </div>
      </div>

      {/* Content */}
      <div className="px-4 py-3">
        {content ? (
          <div className="prose prose-sm max-w-none prose-headings:text-surface-900 prose-p:text-surface-800 prose-a:text-brand-600 prose-a:no-underline hover:prose-a:underline prose-strong:text-surface-900 prose-li:text-surface-800 prose-table:text-surface-800">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeHighlight]}
              components={{
                pre: ({ children }) => (
                  <div className="relative group/pre">
                    <pre className="!m-0 !rounded-lg !bg-surface-950 !p-4 !text-surface-200 overflow-x-auto text-[13px] leading-relaxed">
                      {children}
                    </pre>
                    <CodeBlockCopyButton
                      children={children}
                      copiedBlock={copiedBlock}
                      onCopy={handleCopyBlock}
                      codeBlockId={codeBlockId}
                    />
                  </div>
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
              {content}
            </ReactMarkdown>
            {/* Streaming cursor */}
            {isGenerating && (
              <span className="inline-block w-2 h-4 ml-0.5 bg-brand-500/80 animate-pulse rounded-sm" />
            )}
          </div>
        ) : isGenerating ? (
          <ThinkingIndicator />
        ) : null}
      </div>
    </div>
  );
}

// ── Code block copy button ──

function CodeBlockCopyButton({
  children,
  copiedBlock,
  onCopy,
  codeBlockId,
}: {
  children: React.ReactNode;
  copiedBlock: string | null;
  onCopy: (text: string, id: string) => void;
  codeBlockId: () => string;
}) {
  const idRef = useRef<string>("");

  // Extract text content from code element
  const extractCode = useCallback((node: React.ReactNode): string => {
    if (!node) return "";
    if (typeof node === "string") return node;
    if (Array.isArray(node)) return node.map(extractCode).join("");
    if (node && typeof node === "object" && "props" in node) {
      const props = (node as unknown as Record<string, unknown>).props;
      if (props && typeof props === "object" && "children" in (props as Record<string, unknown>)) {
        return extractCode((props as Record<string, unknown>).children as React.ReactNode);
      }
    }
    return "";
  }, []);

  const id = idRef.current || (idRef.current = codeBlockId());
  const codeText = extractCode(children);

  if (!codeText.trim()) return null;

  return (
    <button
      onClick={() => onCopy(codeText, id)}
      className="absolute top-2 right-2 rounded-md bg-surface-800/60 px-2 py-1 text-[11px] text-surface-300 opacity-0 group-hover/pre:opacity-100 hover:bg-surface-800 transition-all"
      title="Copy code"
    >
      {copiedBlock === id ? <CheckIcon /> : <CopyIcon />}
    </button>
  );
}

// ── Thinking indicator ──

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

// ── Icons ──

function StopIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
      <rect x="6" y="6" width="12" height="12" rx="1" />
    </svg>
  );
}

function CopyIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}
