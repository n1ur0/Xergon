/**
 * ChatMessage -- message bubble component for the chat widget.
 *
 * Renders user/assistant messages with basic markdown support,
 * code block copy, timestamps, and token usage badges.
 */
"use client";

import React, { useState, useCallback } from 'react';
import type { ChatMessage as ChatMessageType } from '../hooks/use-chat';

export interface ChatMessageProps {
  message: ChatMessageType;
  showTimestamp?: boolean;
  showTokenCount?: boolean;
}

/**
 * Basic markdown-to-HTML conversion for chat messages.
 * Supports: **bold**, *italic*, `inline code`, ```code blocks```.
 */
function renderMarkdown(text: string): string {
  let html = text;

  // Escape HTML first
  html = html
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  // Code blocks (```...```)
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, lang, code) => {
    const langLabel = lang ? ` data-lang="${lang}"` : '';
    return `<pre class="xergon-code-block"${langLabel}><code>${code.trim()}</code></pre>`;
  });

  // Inline code (`...`)
  html = html.replace(/`([^`]+)`/g, '<code class="xergon-inline-code">$1</code>');

  // Bold (**...**)
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // Italic (*...*)
  html = html.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '<em>$1</em>');

  // Line breaks
  html = html.replace(/\n/g, '<br/>');

  return html;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback
      const textarea = document.createElement('textarea');
      textarea.value = text;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [text]);

  return (
    <button
      className="xergon-copy-btn"
      onClick={handleCopy}
      title={copied ? 'Copied!' : 'Copy code'}
    >
      {copied ? '✓' : 'Copy'}
    </button>
  );
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function ChatMessageComponent({ message, showTimestamp = true, showTokenCount = false }: ChatMessageProps) {
  const isUser = message.role === 'user';
  const isSystem = message.role === 'system';

  if (isSystem) {
    return null;
  }

  const htmlContent = renderMarkdown(message.content);

  // Extract code blocks for copy buttons
  const codeBlocks: string[] = [];
  const codeBlockRegex = /```(\w*)\n([\s\S]*?)```/g;
  let match;
  while ((match = codeBlockRegex.exec(message.content)) !== null) {
    codeBlocks.push(match[2].trim());
  }

  return (
    <div className={`xergon-message xergon-message-${isUser ? 'user' : 'assistant'}`}>
      <div className="xergon-message-avatar">
        {isUser ? '👤' : '🤖'}
      </div>
      <div className="xergon-message-body">
        <div
          className="xergon-message-content"
          dangerouslySetInnerHTML={{ __html: htmlContent || (message.isStreaming ? '' : '(empty)') }}
        />
        {message.isStreaming && (
          <span className="xergon-typing-indicator">
            <span className="xergon-typing-dot" />
            <span className="xergon-typing-dot" />
            <span className="xergon-typing-dot" />
          </span>
        )}
        <div className="xergon-message-meta">
          {showTimestamp && message.timestamp && (
            <span className="xergon-message-time">{formatTime(message.timestamp)}</span>
          )}
          {message.model && (
            <span className="xergon-message-model">{message.model}</span>
          )}
          {showTokenCount && message.tokens && (
            <span className="xergon-message-tokens">
              {message.tokens.totalTokens} tokens
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

export default ChatMessageComponent;
