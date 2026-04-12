/**
 * ChatWidget -- self-contained embeddable chat widget.
 *
 * Features:
 * - Floating action button toggle
 * - Chat window with header, messages, input
 * - Message bubbles (user right, assistant left)
 * - Streaming indicator (typing animation)
 * - Model selector dropdown
 * - Error display with retry
 * - Clear conversation
 * - Theme support (light/dark/auto)
 * - Customizable colors
 * - Responsive design
 */
"use client";

import React, { useState, useCallback, useEffect, useRef } from 'react';
import { useChat } from '../hooks/use-chat';
import { useModels } from '../hooks/use-models';
import { ChatMessageComponent } from './chat-message';
import { ChatInput } from './chat-input';
import { ModelSelector } from './model-selector';
import type { ChatMessage } from '../hooks/use-chat';
import './styles.css';

// ── Types ────────────────────────────────────────────────────────────

export interface ChatWidgetProps {
  apiKey?: string;
  baseUrl?: string;
  defaultModel?: string;
  systemPrompt?: string;
  theme?: 'light' | 'dark' | 'auto';
  primaryColor?: string;
  position?: 'bottom-right' | 'bottom-left';
  title?: string;
  placeholder?: string;
  welcomeMessage?: string;
  maxHeight?: string;
  maxTokens?: number;
  temperature?: number;
  showModelSelector?: boolean;
  showTokenCount?: boolean;
  onMessage?: (message: ChatMessage) => void;
}

// ── Theme helper ─────────────────────────────────────────────────────

function resolveTheme(theme: 'light' | 'dark' | 'auto'): 'light' | 'dark' {
  if (theme !== 'auto') return theme;
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

// ── Component ────────────────────────────────────────────────────────

export function ChatWidget({
  apiKey,
  baseUrl = 'https://relay.xergon.gg',
  defaultModel = 'default',
  systemPrompt,
  theme = 'auto',
  primaryColor,
  position = 'bottom-right',
  title = 'Xergon Chat',
  placeholder = 'Type a message...',
  welcomeMessage,
  maxHeight = '500px',
  maxTokens,
  temperature,
  showModelSelector = true,
  showTokenCount = false,
  onMessage,
}: ChatWidgetProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [resolvedTheme, setResolvedTheme] = useState(() => resolveTheme(theme));
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const { messages, isLoading, error, send, stop, clear, retry, setModel } = useChat({
    model: defaultModel,
    systemPrompt,
    maxTokens,
    temperature,
    apiKey,
    baseUrl,
    onToken: undefined,
    onComplete: undefined,
    onError: undefined,
  });

  const { models } = useModels({ baseUrl, apiKey });

  // Listen for system theme changes
  useEffect(() => {
    if (theme !== 'auto') {
      setResolvedTheme(theme);
      return;
    }
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setResolvedTheme(e.matches ? 'dark' : 'light');
    mq.addEventListener('change', handler);
    setResolvedTheme(resolveTheme('auto'));
    return () => mq.removeEventListener('change', handler);
  }, [theme]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    if (typeof messagesEndRef.current?.scrollIntoView === 'function') {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  const handleSend = useCallback((content: string) => {
    send(content);
    onMessage?.({
      id: `msg-${Date.now()}`,
      role: 'user',
      content,
      timestamp: new Date(),
    });
  }, [send, onMessage]);

  const handleModelSelect = useCallback((modelId: string) => {
    setModel(modelId);
  }, [setModel]);

  // Build CSS custom properties
  const style: React.CSSProperties = {
    '--xergon-primary': primaryColor || '#6366f1',
    '--xergon-max-height': maxHeight,
  } as React.CSSProperties;

  return (
    <div
      className={`xergon-widget xergon-theme-${resolvedTheme} xergon-pos-${position}`}
      style={style}
    >
      {/* Toggle button (floating action button) */}
      <button
        className="xergon-toggle-btn"
        onClick={() => setIsOpen(!isOpen)}
        aria-label={isOpen ? 'Close chat' : 'Open chat'}
      >
        {isOpen ? (
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        ) : (
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
        )}
      </button>

      {/* Chat window */}
      <div className={`xergon-chat-window ${isOpen ? 'xergon-open' : 'xergon-closed'}`}>
        {/* Header */}
        <div className="xergon-chat-header">
          <div className="xergon-chat-header-left">
            <span className="xergon-chat-header-title">{title}</span>
          </div>
          <div className="xergon-chat-header-right">
            {showModelSelector && models.length > 0 && (
              <ModelSelector
                models={models}
                selectedModel={defaultModel}
                onSelect={handleModelSelect}
              />
            )}
            <button
              className="xergon-header-btn"
              onClick={clear}
              title="Clear conversation"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <path d="M2 4h12M5.33 4V2.67a1.33 1.33 0 011.34-1.34h2.66a1.33 1.33 0 011.34 1.34V4m2 0v9.33a1.33 1.33 0 01-1.34 1.34H4.67a1.33 1.33 0 01-1.34-1.34V4h9.34z" />
              </svg>
            </button>
          </div>
        </div>

        {/* Messages */}
        <div className="xergon-chat-messages">
          {messages.length === 0 && welcomeMessage && (
            <div className="xergon-welcome-message">
              <p>{welcomeMessage}</p>
            </div>
          )}
          {messages.map(msg => (
            <ChatMessageComponent
              key={msg.id}
              message={msg}
              showTimestamp={true}
              showTokenCount={showTokenCount}
            />
          ))}
          {error && (
            <div className="xergon-error-message">
              <span className="xergon-error-text">{error.message}</span>
              <button className="xergon-retry-btn" onClick={retry}>
                Retry
              </button>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* Input */}
        <ChatInput
          onSend={handleSend}
          onStop={stop}
          isLoading={isLoading}
          isStreaming={messages.some(m => m.isStreaming)}
          placeholder={placeholder}
        />
      </div>
    </div>
  );
}

export default ChatWidget;
