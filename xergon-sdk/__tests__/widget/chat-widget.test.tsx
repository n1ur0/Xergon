/**
 * Tests for ChatWidget component.
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatWidget } from '../../src/widget/chat-widget';

// Import the mocked modules
import { useChat } from '../../src/hooks/use-chat';
import { useModels } from '../../src/hooks/use-models';

vi.mock('../../src/hooks/use-chat', () => ({
  useChat: vi.fn(() => ({
    messages: [],
    isLoading: false,
    error: null,
    send: vi.fn(),
    stop: vi.fn(),
    clear: vi.fn(),
    retry: vi.fn(),
    setModel: vi.fn(),
  })),
}));

vi.mock('../../src/hooks/use-models', () => ({
  useModels: vi.fn(() => ({
    models: [
      { id: 'llama-3.3-70b', object: 'model', ownedBy: 'meta', pricing: '0.001' },
      { id: 'mistral-7b', object: 'model', ownedBy: 'mistral' },
    ],
    isLoading: false,
    error: null,
    refetch: vi.fn(),
  })),
}));

const mockedUseChat = vi.mocked(useChat);
const mockedUseModels = vi.mocked(useModels);

describe('ChatWidget', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // Re-apply default mocks
    mockedUseChat.mockReturnValue({
      messages: [],
      isLoading: false,
      error: null,
      send: vi.fn(),
      stop: vi.fn(),
      clear: vi.fn(),
      retry: vi.fn(),
      setModel: vi.fn(),
    });
    mockedUseModels.mockReturnValue({
      models: [
        { id: 'llama-3.3-70b', object: 'model', ownedBy: 'meta', pricing: '0.001' },
        { id: 'mistral-7b', object: 'model', ownedBy: 'mistral' },
      ],
      isLoading: false,
      error: null,
      refetch: vi.fn(),
    });

    // Mock matchMedia
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockImplementation(query => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('renders toggle button', () => {
    it('renders the toggle button', () => {
      render(<ChatWidget />);
      const btn = screen.getByRole('button', { name: /open chat/i });
      expect(btn).toBeInTheDocument();
    });

    it('has correct styling classes', () => {
      const { container } = render(<ChatWidget />);
      const widget = container.querySelector('.xergon-widget');
      expect(widget).toBeInTheDocument();
      expect(widget).toHaveClass('xergon-theme-light');
      expect(widget).toHaveClass('xergon-pos-bottom-right');
    });
  });

  describe('opens chat window on toggle', () => {
    it('toggles chat window open/close', () => {
      const { container } = render(<ChatWidget title="Test Chat" />);

      const toggleBtn = screen.getByRole('button', { name: /open chat/i });

      // Initially closed (window has closed class)
      const chatWindow = container.querySelector('.xergon-chat-window');
      expect(chatWindow).toHaveClass('xergon-closed');

      // Click to open
      fireEvent.click(toggleBtn);
      expect(chatWindow).toHaveClass('xergon-open');
      expect(screen.getByText('Test Chat')).toBeInTheDocument();

      // Click to close
      const closeBtn = screen.getByRole('button', { name: /close chat/i });
      fireEvent.click(closeBtn);
      expect(chatWindow).toHaveClass('xergon-closed');
    });
  });

  describe('theme application', () => {
    it('applies dark theme', () => {
      const { container } = render(<ChatWidget theme="dark" />);
      const widget = container.querySelector('.xergon-widget');
      expect(widget).toHaveClass('xergon-theme-dark');
    });

    it('applies light theme', () => {
      const { container } = render(<ChatWidget theme="light" />);
      const widget = container.querySelector('.xergon-widget');
      expect(widget).toHaveClass('xergon-theme-light');
    });
  });

  describe('position', () => {
    it('applies bottom-left position', () => {
      const { container } = render(<ChatWidget position="bottom-left" />);
      const widget = container.querySelector('.xergon-widget');
      expect(widget).toHaveClass('xergon-pos-bottom-left');
    });
  });

  describe('clear button', () => {
    it('renders clear button when chat is open', () => {
      render(<ChatWidget />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      const clearBtn = screen.getByTitle('Clear conversation');
      expect(clearBtn).toBeInTheDocument();
    });
  });

  describe('welcome message', () => {
    it('shows welcome message when chat is open and no messages', () => {
      render(<ChatWidget welcomeMessage="Welcome to Xergon!" />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      expect(screen.getByText('Welcome to Xergon!')).toBeInTheDocument();
    });
  });

  describe('model selector', () => {
    it('shows model selector when showModelSelector is true', () => {
      render(<ChatWidget showModelSelector={true} />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      const selectorBtn = screen.getByTitle('Select model');
      expect(selectorBtn).toBeInTheDocument();
    });

    it('hides model selector when showModelSelector is false', () => {
      render(<ChatWidget showModelSelector={false} />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      expect(screen.queryByTitle('Select model')).not.toBeInTheDocument();
    });
  });

  describe('renders messages', () => {
    it('renders user and assistant messages', () => {
      mockedUseChat.mockReturnValue({
        messages: [
          {
            id: 'msg-1',
            role: 'user' as const,
            content: 'Hello!',
            timestamp: new Date('2024-01-01T12:00:00'),
          },
          {
            id: 'msg-2',
            role: 'assistant' as const,
            content: 'Hi there! How can I help?',
            timestamp: new Date('2024-01-01T12:00:01'),
            model: 'llama-3.3-70b',
          },
        ],
        isLoading: false,
        error: null,
        send: vi.fn(),
        stop: vi.fn(),
        clear: vi.fn(),
        retry: vi.fn(),
        setModel: vi.fn(),
      });

      render(<ChatWidget />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      expect(screen.getByText('Hello!')).toBeInTheDocument();
      expect(screen.getByText('Hi there! How can I help?')).toBeInTheDocument();
    });
  });

  describe('error display', () => {
    it('shows error message and retry button', () => {
      const mockRetry = vi.fn();
      mockedUseChat.mockReturnValue({
        messages: [],
        isLoading: false,
        error: new Error('Something went wrong'),
        send: vi.fn(),
        stop: vi.fn(),
        clear: vi.fn(),
        retry: mockRetry,
        setModel: vi.fn(),
      });

      render(<ChatWidget />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      expect(screen.getByText('Something went wrong')).toBeInTheDocument();
      const retryBtn = screen.getByText('Retry');
      expect(retryBtn).toBeInTheDocument();

      fireEvent.click(retryBtn);
      expect(mockRetry).toHaveBeenCalledTimes(1);
    });
  });

  describe('chat input', () => {
    it('renders input textarea when chat is open', () => {
      render(<ChatWidget placeholder="Type here..." />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      const textarea = screen.getByPlaceholderText('Type here...');
      expect(textarea).toBeInTheDocument();
    });

    it('renders send button', () => {
      render(<ChatWidget />);

      // Open the chat
      fireEvent.click(screen.getByRole('button', { name: /open chat/i }));

      expect(screen.getByTitle(/send message/i)).toBeInTheDocument();
    });
  });
});
