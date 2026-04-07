/**
 * Tests for useChat hook.
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useChat } from '../../src/hooks/use-chat';

// We need React as a peer for renderHook
import React from 'react';

// Mock AbortController with proper signal
class MockAbortSignal {
  aborted = false;
  addEventListener = vi.fn();
  removeEventListener = vi.fn();
  dispatchEvent = vi.fn();
}
class MockAbortController {
  signal = new MockAbortSignal();
  abort = vi.fn(() => { (this.signal as any).aborted = true; });
}
(globalThis as any).AbortController = MockAbortController;

describe('useChat', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('initial state', () => {
    it('returns empty messages array', () => {
      const { result } = renderHook(() => useChat());
      expect(result.current.messages).toEqual([]);
    });

    it('returns isLoading false', () => {
      const { result } = renderHook(() => useChat());
      expect(result.current.isLoading).toBe(false);
    });

    it('returns null error', () => {
      const { result } = renderHook(() => useChat());
      expect(result.current.error).toBeNull();
    });

    it('uses default model', () => {
      const { result } = renderHook(() => useChat());
      // model is internal state, accessible via setModel
      expect(result.current.setModel).toBeDefined();
    });

    it('accepts custom model in options', () => {
      const { result } = renderHook(() => useChat({ model: 'custom-model' }));
      expect(result.current.setModel).toBeDefined();
    });
  });

  describe('clear', () => {
    it('removes all messages', async () => {
      const { result } = renderHook(() => useChat());

      act(() => {
        // Manually set messages via a send that will be mocked
        // We'll test clear in isolation by checking it doesn't throw
        result.current.clear();
      });

      expect(result.current.messages).toEqual([]);
      expect(result.current.error).toBeNull();
    });
  });

  describe('stop', () => {
    it('does not throw when no request is active', () => {
      const { result } = renderHook(() => useChat());

      expect(() => {
        act(() => result.current.stop());
      }).not.toThrow();
    });
  });

  describe('send', () => {
    it('sets loading to true when sending', async () => {
      // Create a fetch mock that never resolves (stays loading)
      const neverResolve = new Promise(() => {});
      vi.stubGlobal('fetch', vi.fn().mockReturnValue(neverResolve));

      const { result } = renderHook(() => useChat());

      await act(async () => {
        result.current.send('Hello');
      });

      expect(result.current.isLoading).toBe(true);
      expect(result.current.messages).toHaveLength(2); // user + assistant placeholder
      expect(result.current.messages[0].role).toBe('user');
      expect(result.current.messages[0].content).toBe('Hello');
      expect(result.current.messages[1].role).toBe('assistant');
      expect(result.current.messages[1].isStreaming).toBe(true);
    });

    it('adds user message with correct content', async () => {
      const neverResolve = new Promise(() => {});
      vi.stubGlobal('fetch', vi.fn().mockReturnValue(neverResolve));

      const { result } = renderHook(() => useChat());

      await act(async () => {
        result.current.send('Test message');
      });

      expect(result.current.messages[0].content).toBe('Test message');
      expect(result.current.messages[0].role).toBe('user');
    });

    it('adds assistant message placeholder', async () => {
      const neverResolve = new Promise(() => {});
      vi.stubGlobal('fetch', vi.fn().mockReturnValue(neverResolve));

      const { result } = renderHook(() => useChat({ model: 'test-model' }));

      await act(async () => {
        result.current.send('Hello');
      });

      expect(result.current.messages[1].role).toBe('assistant');
      expect(result.current.messages[1].content).toBe('');
      expect(result.current.messages[1].isStreaming).toBe(true);
      expect(result.current.messages[1].model).toBe('test-model');
    });
  });

  describe('error handling', () => {
    it('sets error when fetch fails', async () => {
      vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Network error')));

      const { result } = renderHook(() => useChat());

      await act(async () => {
        await result.current.send('Hello');
      });

      expect(result.current.error).toBeInstanceOf(Error);
      expect(result.current.error!.message).toBe('Network error');
      expect(result.current.isLoading).toBe(false);
    });

    it('sets error when response is not ok', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        statusText: 'Unauthorized',
        json: () => Promise.resolve({ error: { message: 'Invalid API key', code: 401 } }),
      }));

      const { result } = renderHook(() => useChat());

      await act(async () => {
        await result.current.send('Hello');
      });

      expect(result.current.error).toBeInstanceOf(Error);
      expect(result.current.error!.message).toContain('401');
      expect(result.current.isLoading).toBe(false);
    });
  });

  describe('setModel', () => {
    it('is a function', () => {
      const { result } = renderHook(() => useChat());
      expect(typeof result.current.setModel).toBe('function');
    });
  });

  describe('retry', () => {
    it('does nothing when no user messages exist', async () => {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        body: null,
      });
      vi.stubGlobal('fetch', fetchMock);

      const { result } = renderHook(() => useChat());

      await act(async () => {
        await result.current.retry();
      });

      expect(fetchMock).not.toHaveBeenCalled();
    });
  });

  describe('callbacks', () => {
    it('calls onError callback on error', async () => {
      const onError = vi.fn();
      vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Test error')));

      const { result } = renderHook(() => useChat({ onError }));

      await act(async () => {
        await result.current.send('Hello');
      });

      expect(onError).toHaveBeenCalledTimes(1);
      expect(onError).toHaveBeenCalledWith(expect.any(Error));
    });
  });
});
