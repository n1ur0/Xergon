"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.min.css";
import { useWidgetChat, type WidgetMessage } from "@/lib/embed/use-widget-chat";
import { sanitizeColor, type WidgetConfig } from "@/lib/embed/config";

// ── Types ──

export interface XergonWidgetProps extends WidgetConfig {
  apiBase?: string;
}

// ── Inline styles helper (no Tailwind dependency for standalone) ──

function styles(primaryColor: string) {
  return {
    container: {
      fontFamily:
        '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
      position: "fixed" as const,
      bottom: 24,
      right: 24,
      zIndex: 2147483647,
      display: "flex",
      flexDirection: "column" as const,
      alignItems: "flex-end",
    },
    containerLeft: {
      right: "auto",
      left: 24,
      alignItems: "flex-start",
    },
    bubble: {
      width: 60,
      height: 60,
      borderRadius: "50%",
      backgroundColor: primaryColor,
      border: "none",
      cursor: "pointer",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
      transition: "transform 0.2s, box-shadow 0.2s",
    },
    bubbleHover: {
      transform: "scale(1.05)",
      boxShadow: "0 6px 20px rgba(0,0,0,0.2)",
    },
    panel: {
      width: 380,
      maxWidth: "calc(100vw - 32px)",
      height: 520,
      maxHeight: "calc(100vh - 100px)",
      borderRadius: 16,
      backgroundColor: "#ffffff",
      boxShadow: "0 8px 32px rgba(0,0,0,0.12), 0 2px 8px rgba(0,0,0,0.08)",
      display: "flex",
      flexDirection: "column" as const,
      overflow: "hidden",
      border: "1px solid #e5e7eb",
    },
    header: {
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "14px 16px",
      backgroundColor: primaryColor,
      color: "#ffffff",
      flexShrink: 0,
    },
    headerTitle: {
      fontSize: 15,
      fontWeight: 600,
      margin: 0,
    },
    closeBtn: {
      background: "rgba(255,255,255,0.2)",
      border: "none",
      borderRadius: "50%",
      width: 28,
      height: 28,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      cursor: "pointer",
      color: "#ffffff",
      fontSize: 18,
      lineHeight: 1,
    },
    messagesArea: {
      flex: 1,
      overflowY: "auto" as const,
      padding: 16,
      display: "flex",
      flexDirection: "column" as const,
      gap: 12,
      backgroundColor: "#f9fafb",
    },
    userMsg: {
      alignSelf: "flex-end" as const,
      maxWidth: "80%",
      backgroundColor: primaryColor,
      color: "#ffffff",
      padding: "10px 14px",
      borderRadius: 16,
      borderBottomRightRadius: 4,
      fontSize: 14,
      lineHeight: 1.5,
      wordBreak: "break-word" as const,
      whiteSpace: "pre-wrap" as const,
    },
    assistantMsg: {
      alignSelf: "flex-start" as const,
      maxWidth: "85%",
      backgroundColor: "#ffffff",
      color: "#1f2937",
      padding: "10px 14px",
      borderRadius: 16,
      borderBottomLeftRadius: 4,
      fontSize: 14,
      lineHeight: 1.6,
      wordBreak: "break-word" as const,
      border: "1px solid #e5e7eb",
    },
    errorMsg: {
      backgroundColor: "#fef2f2",
      borderColor: "#fecaca",
      color: "#991b1b",
    },
    welcomeMsg: {
      alignSelf: "center" as const,
      maxWidth: "90%",
      textAlign: "center" as const,
      color: "#6b7280",
      fontSize: 13,
      padding: "8px 0",
    },
    inputArea: {
      padding: 12,
      borderTop: "1px solid #e5e7eb",
      backgroundColor: "#ffffff",
      flexShrink: 0,
    },
    inputRow: {
      display: "flex",
      gap: 8,
      alignItems: "flex-end",
    },
    textarea: {
      flex: 1,
      border: "1px solid #d1d5db",
      borderRadius: 12,
      padding: "10px 14px",
      fontSize: 14,
      fontFamily: "inherit",
      resize: "none" as const,
      outline: "none",
      minHeight: 40,
      maxHeight: 120,
      lineHeight: 1.5,
    },
    textareaFocus: {
      borderColor: primaryColor,
      boxShadow: `0 0 0 2px ${primaryColor}33`,
    },
    sendBtn: {
      width: 40,
      height: 40,
      borderRadius: "50%",
      backgroundColor: primaryColor,
      border: "none",
      cursor: "pointer",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      flexShrink: 0,
      transition: "opacity 0.2s",
    },
    sendBtnDisabled: {
      opacity: 0.4,
      cursor: "not-allowed",
    },
    stopBtn: {
      backgroundColor: "#ef4444",
    },
    footer: {
      padding: "8px 12px",
      borderTop: "1px solid #f3f4f6",
      textAlign: "center" as const,
      flexShrink: 0,
    },
    footerLink: {
      fontSize: 11,
      color: "#9ca3af",
      textDecoration: "none",
    },
    // Markdown overrides
    markdown: {
      fontSize: 14,
      lineHeight: 1.6,
    },
    markdownPre: {
      margin: "8px 0",
      borderRadius: 8,
      backgroundColor: "#1e1e2e",
      padding: 12,
      overflowX: "auto" as const,
      fontSize: 13,
    },
    markdownCode: {
      backgroundColor: "#f3f4f6",
      padding: "1px 5px",
      borderRadius: 4,
      fontSize: 13,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      color: primaryColor,
    },
    markdownTable: {
      borderCollapse: "collapse" as const,
      width: "100%",
      fontSize: 13,
    },
    markdownTd: {
      border: "1px solid #e5e7eb",
      padding: "6px 10px",
    },
    loadingDots: {
      display: "flex",
      gap: 4,
      alignItems: "center",
      padding: "4px 0",
    },
    dot: {
      width: 6,
      height: 6,
      borderRadius: "50%",
      backgroundColor: "#9ca3af",
    },
  };
}

// ── Component ──

export function XergonWidget({
  model = "",
  welcomeMessage = "Hello! How can I help you today?",
  primaryColor = "#6366f1",
  position = "bottom-right",
  title = "Xergon Chat",
  publicKey,
  apiBase,
}: XergonWidgetProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [input, setInput] = useState("");
  const [textareaFocused, setTextareaFocused] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const resolvedApiBase = apiBase || (typeof window !== "undefined" && (window as unknown as Record<string, string>).__XERGON_API_BASE) || "/v1";

  const { messages, isGenerating, sendMessage, stopGeneration } = useWidgetChat({
    model,
    publicKey,
    apiBase: resolvedApiBase,
  });

  const s = styles(primaryColor);
  const isLeft = position === "bottom-left";

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isGenerating]);

  // Auto-resize textarea
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 120) + "px";
  }, [input]);

  // Focus textarea when opening
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => textareaRef.current?.focus(), 100);
    }
  }, [isOpen]);

  // Notify parent iframe of size changes
  useEffect(() => {
    if (isOpen && window.parent !== window) {
      window.parent.postMessage(
        { type: "xergon-widget-resize", height: 600, open: true },
        "*",
      );
    } else if (!isOpen && window.parent !== window) {
      window.parent.postMessage(
        { type: "xergon-widget-resize", height: 80, open: false },
        "*",
      );
    }
  }, [isOpen]);

  const handleSend = useCallback(async () => {
    if (!input.trim() || isGenerating) return;
    const msg = input;
    setInput("");
    if (textareaRef.current) textareaRef.current.style.height = "auto";
    await sendMessage(msg);
  }, [input, isGenerating, sendMessage]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  return (
    <div style={isLeft ? { ...s.container, ...s.containerLeft } : s.container}>
      {/* Chat Panel */}
      {isOpen && (
        <div style={s.panel}>
          {/* Header */}
          <div style={s.header}>
            <h3 style={s.headerTitle}>{title}</h3>
            <button
              style={s.closeBtn}
              onClick={() => setIsOpen(false)}
              aria-label="Close chat"
            >
              ✕
            </button>
          </div>

          {/* Messages */}
          <div style={s.messagesArea}>
            {messages.length === 0 && !isGenerating && (
              <div style={s.welcomeMsg}>{welcomeMessage}</div>
            )}

            {messages.map((msg: WidgetMessage) => (
              <div
                key={msg.id}
                style={{
                  ...(msg.role === "user" ? s.userMsg : s.assistantMsg),
                  ...(msg.isError ? s.errorMsg : {}),
                }}
              >
                {msg.role === "assistant" ? (
                  msg.content ? (
                    <div style={s.markdown}>
                      <ReactMarkdown
                        remarkPlugins={[remarkGfm]}
                        rehypePlugins={[rehypeHighlight]}
                        components={{
                          pre: ({ children }) => (
                            <pre style={s.markdownPre}>{children}</pre>
                          ),
                          code: ({ className, children, ...props }) => {
                            const isInline = !className;
                            if (isInline) {
                              return (
                                <code style={s.markdownCode} {...props}>
                                  {children}
                                </code>
                              );
                            }
                            return <code className={className} {...props}>{children}</code>;
                          },
                          table: ({ children }) => (
                            <div style={{ overflowX: "auto" }}>
                              <table style={s.markdownTable}>{children}</table>
                            </div>
                          ),
                          td: ({ children }) => (
                            <td style={s.markdownTd}>{children}</td>
                          ),
                          th: ({ children }) => (
                            <td style={{ ...s.markdownTd, fontWeight: 600, backgroundColor: "#f9fafb" }}>{children}</td>
                          ),
                        }}
                      >
                        {msg.content}
                      </ReactMarkdown>
                    </div>
                  ) : isGenerating ? (
                    <ThinkingDots color={primaryColor} />
                  ) : null
                ) : (
                  <span>{msg.content}</span>
                )}
              </div>
            ))}

            {isGenerating && messages.length > 0 && messages[messages.length - 1].role === "assistant" && messages[messages.length - 1].content && (
              <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "2px 4px" }}>
                <span style={{ width: 6, height: 6, borderRadius: "50%", backgroundColor: primaryColor, animation: "pulse 1.5s infinite" }} />
                <span style={{ fontSize: 12, color: "#9ca3af" }}>Streaming...</span>
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input */}
          <div style={s.inputArea}>
            <div style={s.inputRow}>
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                onFocus={() => setTextareaFocused(true)}
                onBlur={() => setTextareaFocused(false)}
                placeholder="Type a message..."
                rows={1}
                disabled={isGenerating}
                style={{
                  ...s.textarea,
                  ...(textareaFocused ? s.textareaFocus : {}),
                }}
              />
              {isGenerating ? (
                <button
                  style={{ ...s.sendBtn, ...s.stopBtn }}
                  onClick={stopGeneration}
                  aria-label="Stop generating"
                >
                  <StopIcon />
                </button>
              ) : (
                <button
                  style={{
                    ...s.sendBtn,
                    ...((!input.trim() || !model) ? s.sendBtnDisabled : {}),
                  }}
                  onClick={handleSend}
                  disabled={!input.trim() || !model}
                  aria-label="Send message"
                >
                  <SendIcon />
                </button>
              )}
            </div>
          </div>

          {/* Footer */}
          <div style={s.footer}>
            <a
              href="https://xergon.network"
              target="_blank"
              rel="noopener noreferrer"
              style={s.footerLink}
            >
              Powered by Xergon
            </a>
          </div>
        </div>
      )}

      {/* Floating Button */}
      {!isOpen && (
        <button
          style={s.bubble}
          onClick={() => setIsOpen(true)}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(1.05)";
            (e.currentTarget as HTMLElement).style.boxShadow = "0 6px 20px rgba(0,0,0,0.2)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.transform = "scale(1)";
            (e.currentTarget as HTMLElement).style.boxShadow = "0 4px 12px rgba(0,0,0,0.15)";
          }}
          aria-label="Open chat"
        >
          <ChatIcon />
        </button>
      )}
    </div>
  );
}

// ── Inline SVG Icons ──

function ChatIcon() {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    </svg>
  );
}

function SendIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 12h14" />
      <path d="m12 5 7 7-7 7" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="white" stroke="none">
      <rect x="6" y="6" width="12" height="12" rx="2" />
    </svg>
  );
}

function ThinkingDots({ color }: { color: string }) {
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center", padding: "4px 0" }}>
      {[0, 150, 300].map((delay, i) => (
        <span
          key={i}
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            backgroundColor: color || "#9ca3af",
            opacity: 0.5,
            animation: `xergon-bounce 1.4s infinite ease-in-out ${delay}ms`,
          }}
        />
      ))}
      <style>{`
        @keyframes xergon-bounce {
          0%, 80%, 100% { transform: translateY(0); opacity: 0.4; }
          40% { transform: translateY(-6px); opacity: 1; }
        }
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }
      `}</style>
    </div>
  );
}
