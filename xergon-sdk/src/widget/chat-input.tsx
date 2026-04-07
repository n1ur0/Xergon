/**
 * ChatInput -- input component for the chat widget.
 *
 * Features: auto-resize textarea, send/stop buttons,
 * keyboard shortcuts (Enter to send, Shift+Enter for newline).
 */

import React, { useState, useRef, useCallback, useEffect } from 'react';

export interface ChatInputProps {
  onSend: (content: string) => void;
  onStop?: () => void;
  isLoading?: boolean;
  isStreaming?: boolean;
  placeholder?: string;
  disabled?: boolean;
}

export function ChatInput({
  onSend,
  onStop,
  isLoading = false,
  isStreaming = false,
  placeholder = 'Type a message...',
  disabled = false,
}: ChatInputProps) {
  const [value, setValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
  }, [value]);

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue('');
    // Reset height
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [value, disabled, onSend]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  const canSend = value.trim().length > 0 && !disabled && !isLoading;

  return (
    <div className="xergon-chat-input">
      <textarea
        ref={textareaRef}
        className="xergon-textarea"
        value={value}
        onChange={e => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        disabled={disabled || isLoading}
        rows={1}
      />
      <div className="xergon-input-actions">
        {isStreaming ? (
          <button
            className="xergon-btn xergon-btn-stop"
            onClick={onStop}
            title="Stop generating"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <rect x="3" y="3" width="10" height="10" rx="1" />
            </svg>
            Stop
          </button>
        ) : (
          <button
            className="xergon-btn xergon-btn-send"
            onClick={handleSend}
            disabled={!canSend}
            title="Send message (Enter)"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M1 1l14 7-14 7V9l10-2-10-2V1z" />
            </svg>
            Send
          </button>
        )}
      </div>
    </div>
  );
}

export default ChatInput;
