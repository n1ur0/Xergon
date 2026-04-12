/**
 * useChat -- React hook for streaming chat completions.
 *
 * Manages conversation state, streaming responses, abort, retry,
 * and integrates with the Xergon SDK chat API.
 */
"use client";

import { useState, useCallback, useRef, useEffect } from 'react';
import type { RetryConfig } from '../retry';
import type { Model } from '../types';

// ── Types ────────────────────────────────────────────────────────────

export interface UseChatOptions {
  /** Default model to use for completions. */
  model?: string;
  /** System prompt prepended to every request. */
  systemPrompt?: string;
  /** Maximum tokens for completions. */
  maxTokens?: number;
  /** Sampling temperature (0-2). */
  temperature?: number;
  /** API key for authentication. */
  apiKey?: string;
  /** Base URL of the Xergon relay. */
  baseUrl?: string;
  /** Called for each streamed token. */
  onToken?: (token: string) => void;
  /** Called when a completion finishes streaming. */
  onComplete?: (fullResponse: string) => void;
  /** Called when an error occurs. */
  onError?: (error: Error) => void;
  /** Retry configuration for failed requests. */
  retryConfig?: RetryConfig;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
  model?: string;
  tokens?: TokenUsage;
  isStreaming?: boolean;
}

export interface TokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

// ── Helpers ──────────────────────────────────────────────────────────

function generateId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function buildMessages(
  history: ChatMessage[],
  systemPrompt?: string,
): { role: 'system' | 'user' | 'assistant'; content: string }[] {
  const messages: { role: 'system' | 'user' | 'assistant'; content: string }[] = [];
  if (systemPrompt) {
    messages.push({ role: 'system', content: systemPrompt });
  }
  for (const msg of history) {
    if (msg.content || msg.role === 'user') {
      messages.push({ role: msg.role, content: msg.content });
    }
  }
  return messages;
}

// ── Hook ─────────────────────────────────────────────────────────────

export function useChat(options: UseChatOptions = {}) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [model, setModel] = useState(options.model || 'default');
  const abortRef = useRef<AbortController | null>(null);
  const messagesRef = useRef(messages);
  const optionsRef = useRef(options);

  // Keep refs in sync
  useEffect(() => { messagesRef.current = messages; }, [messages]);
  useEffect(() => { optionsRef.current = options; }, [options]);

  /**
   * Send a message and stream the response.
   */
  const send = useCallback(async (
    content: string,
    opts?: { model?: string; systemPrompt?: string },
  ) => {
    const currentModel = opts?.model || model;
    const currentSystemPrompt = opts?.systemPrompt ?? optionsRef.current.systemPrompt;
    const baseUrl = optionsRef.current.baseUrl || 'https://relay.xergon.gg';
    const apiKey = optionsRef.current.apiKey;
    const maxTokens = optionsRef.current.maxTokens;
    const temperature = optionsRef.current.temperature;

    setIsLoading(true);
    setError(null);

    const userMsg: ChatMessage = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: new Date(),
    };

    const assistantMsg: ChatMessage = {
      id: generateId(),
      role: 'assistant',
      content: '',
      timestamp: new Date(),
      model: currentModel,
      isStreaming: true,
    };

    setMessages(prev => [...prev, userMsg, assistantMsg]);

    try {
      abortRef.current = new AbortController();

      const apiMessages = buildMessages(
        [...messagesRef.current, userMsg],
        currentSystemPrompt,
      );

      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        'Accept': 'text/event-stream',
      };
      if (apiKey) {
        headers['Authorization'] = `Bearer ${apiKey}`;
      }

      const res = await fetch(`${baseUrl}/v1/chat/completions`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          model: currentModel,
          messages: apiMessages,
          stream: true,
          ...(maxTokens != null ? { max_tokens: maxTokens } : {}),
          ...(temperature != null ? { temperature } : {}),
        }),
        signal: abortRef.current.signal,
      });

      if (!res.ok) {
        let errorData: unknown;
        try { errorData = await res.json(); } catch { errorData = { message: res.statusText }; }
        const errMsg = errorData && typeof errorData === 'object' && 'error' in errorData
          ? String((errorData as { error: { message?: string } }).error?.message ?? res.statusText)
          : res.statusText;
        throw new Error(`Chat request failed (${res.status}): ${errMsg}`);
      }

      if (!res.body) {
        throw new Error('Response body is not readable -- streaming not supported');
      }

      // Read SSE stream
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let fullContent = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        // Parse SSE events from buffer
        const eventEnd = '\n\n';
        let eventIdx = buffer.indexOf(eventEnd);
        while (eventIdx !== -1) {
          const eventBlock = buffer.substring(0, eventIdx + eventEnd.length);
          buffer = buffer.substring(eventIdx + eventEnd.length);

          for (const line of eventBlock.split('\n')) {
            if (!line.startsWith('data: ')) continue;
            const data = line.slice(6).trim();
            if (data === '[DONE]') continue;

            try {
              const chunk = JSON.parse(data);
              const delta = chunk.choices?.[0]?.delta;
              if (delta?.content) {
                fullContent += delta.content;
                const currentContent = fullContent;

                setMessages(prev => prev.map(m =>
                  m.id === assistantMsg.id
                    ? { ...m, content: currentContent }
                    : m,
                ));

                optionsRef.current.onToken?.(delta.content);
              }
            } catch {
              // Skip malformed JSON
            }
          }

          eventIdx = buffer.indexOf(eventEnd);
        }
      }

      // Mark streaming complete
      setMessages(prev => prev.map(m =>
        m.id === assistantMsg.id
          ? { ...m, isStreaming: false }
          : m,
      ));

      optionsRef.current.onComplete?.(fullContent);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        // User cancelled -- mark streaming stopped
        setMessages(prev => prev.map(m =>
          m.id === assistantMsg.id
            ? { ...m, isStreaming: false }
            : m,
        ));
      } else {
        const error = err instanceof Error ? err : new Error(String(err));
        setError(error);
        optionsRef.current.onError?.(error);

        // Remove the empty assistant message on error
        setMessages(prev => prev.filter(m => !(m.id === assistantMsg.id && m.content === '')));
      }
    } finally {
      setIsLoading(false);
      abortRef.current = null;
    }
  }, [model]);

  /**
   * Abort the current streaming request.
   */
  const stop = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
  }, []);

  /**
   * Clear all messages and error state.
   */
  const clear = useCallback(() => {
    setMessages([]);
    setError(null);
    stop();
  }, [stop]);

  /**
   * Retry the last user message (removes last assistant error message).
   */
  const retry = useCallback(async () => {
    const msgs = messagesRef.current;
    // Find the last user message
    const lastUser = [...msgs].reverse().find(m => m.role === 'user');
    if (!lastUser) return;

    // Remove the last assistant message (error)
    setMessages(prev => {
      const last = prev[prev.length - 1];
      if (last?.role === 'assistant' && last.content === '') {
        return prev.slice(0, -1);
      }
      return prev;
    });

    await send(lastUser.content);
  }, [send]);

  return {
    messages,
    isLoading,
    error,
    send,
    stop,
    clear,
    retry,
    setModel,
  } as const;
}
