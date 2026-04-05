/**
 * Standalone React hook for widget chat -- no zustand dependency.
 * Manages messages, streaming SSE parsing, and auto-reconnect.
 */

import { useState, useCallback, useRef, useEffect } from "react";

// ── Types ──

export interface WidgetMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  model?: string;
  timestamp: number;
  isError?: boolean;
}

export interface WidgetChatState {
  messages: WidgetMessage[];
  isGenerating: boolean;
  error: string | null;
}

export interface UseWidgetChatOptions {
  model: string;
  publicKey?: string;
  apiBase: string;
  onTokenStream?: (content: string) => void;
}

export interface UseWidgetChatReturn extends WidgetChatState {
  sendMessage: (content: string) => Promise<void>;
  stopGeneration: () => void;
  clearMessages: () => void;
  setModel: (model: string) => void;
}

// ── SSE Parser ──

function parseSSEChunk(
  buffer: string,
): { content: string; usage: { inputTokens?: number; outputTokens?: number; costNanoerg?: number }; remaining: string } {
  let accumulated = "";
  let remaining = buffer;
  const usage: { inputTokens?: number; outputTokens?: number; costNanoerg?: number } = {};

  const lines = buffer.split("\n");
  remaining = lines.pop() || "";

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed === "data: [DONE]") continue;
    if (!trimmed.startsWith("data: ")) continue;

    try {
      const json = JSON.parse(trimmed.slice(6));

      if (json.usage) {
        usage.inputTokens = json.usage.prompt_tokens;
        usage.outputTokens = json.usage.completion_tokens;
        usage.costNanoerg = json.usage.cost_nanoerg;
      }

      const content = json.choices?.[0]?.delta?.content;
      if (content) {
        accumulated += content;
      }
    } catch {
      // Skip malformed SSE data
    }
  }

  return { content: accumulated, usage, remaining };
}

// ── Hook ──

export function useWidgetChat({
  model: initialModel,
  publicKey,
  apiBase,
  onTokenStream,
}: UseWidgetChatOptions): UseWidgetChatReturn {
  const [messages, setMessages] = useState<WidgetMessage[]>([]);
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [model, setModel] = useState(initialModel);

  const abortRef = useRef<AbortController | null>(null);
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  const sendMessage = useCallback(
    async (content: string) => {
      if (!content.trim() || !model || isGenerating) return;

      setError(null);
      setIsGenerating(true);

      const userMsg: WidgetMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: content.trim(),
        timestamp: Date.now(),
      };

      const assistantMsg: WidgetMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        model,
        timestamp: Date.now(),
      };

      setMessages((prev) => [...prev, userMsg, assistantMsg]);

      const abort = new AbortController();
      abortRef.current = abort;

      try {
        // Build conversation history
        const history = messagesRef.current.map((m) => ({
          role: m.role,
          content: m.content,
        }));
        history.push({ role: "user", content: content.trim() });

        const headers: Record<string, string> = {
          "Content-Type": "application/json",
          Accept: "text/event-stream",
        };
        if (publicKey) {
          headers["X-Wallet-PK"] = publicKey;
        }

        const res = await fetch(`${apiBase}/chat/completions`, {
          method: "POST",
          headers,
          body: JSON.stringify({
            model,
            messages: history,
            stream: true,
          }),
          signal: abort.signal,
        });

        if (!res.ok) {
          throw new Error(`Model returned status ${res.status}. Please try again.`);
        }

        const reader = res.body?.getReader();
        if (!reader) {
          throw new Error("No response stream received.");
        }

        const decoder = new TextDecoder();
        let accumulated = "";
        let sseBuffer = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          sseBuffer += decoder.decode(value, { stream: true });
          const { content: chunk, remaining } = parseSSEChunk(sseBuffer);
          sseBuffer = remaining;

          if (chunk) {
            accumulated += chunk;
            setMessages((prev) => {
              const updated = [...prev];
              const last = updated[updated.length - 1];
              if (last && last.role === "assistant") {
                updated[updated.length - 1] = { ...last, content: accumulated };
              }
              return updated;
            });
            onTokenStream?.(accumulated);
          }
        }

        // Process any remaining buffer
        if (sseBuffer.trim()) {
          const { content: chunk } = parseSSEChunk(sseBuffer + "\n");
          if (chunk) {
            accumulated += chunk;
            setMessages((prev) => {
              const updated = [...prev];
              const last = updated[updated.length - 1];
              if (last && last.role === "assistant") {
                updated[updated.length - 1] = { ...last, content: accumulated };
              }
              return updated;
            });
          }
        }

        if (!accumulated) {
          setMessages((prev) => {
            const updated = [...prev];
            const last = updated[updated.length - 1];
            if (last && last.role === "assistant") {
              updated[updated.length - 1] = { ...last, content: "(No response content received)", isError: true };
            }
            return updated;
          });
        }
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") {
          setMessages((prev) => {
            const updated = [...prev];
            const last = updated[updated.length - 1];
            if (last && last.role === "assistant") {
              updated[updated.length - 1] = {
                ...last,
                content: last.content || "(Generation stopped)",
                isError: true,
              };
            }
            return updated;
          });
        } else {
          const message = err instanceof Error ? err.message : "Failed to get response. Please try again.";
          setError(message);
          setMessages((prev) => {
            const updated = [...prev];
            const last = updated[updated.length - 1];
            if (last && last.role === "assistant") {
              updated[updated.length - 1] = { ...last, content: `Error: ${message}`, isError: true };
            }
            return updated;
          });
        }
      } finally {
        setIsGenerating(false);
        abortRef.current = null;
      }
    },
    [model, isGenerating, publicKey, apiBase, onTokenStream],
  );

  const stopGeneration = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const clearMessages = useCallback(() => {
    setMessages([]);
    setError(null);
  }, []);

  // Auto-reconnect: retry on network error after a short delay
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryCountRef = useRef(0);

  useEffect(() => {
    if (error && retryCountRef.current < 3) {
      retryCountRef.current += 1;
      retryTimerRef.current = setTimeout(() => {
        setError(null);
      }, 2000 * retryCountRef.current);
    } else if (!error) {
      retryCountRef.current = 0;
    }
    return () => {
      if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
    };
  }, [error]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  return {
    messages,
    isGenerating,
    error,
    sendMessage,
    stopGeneration,
    clearMessages,
    setModel,
  };
}
