/**
 * Tests for RequestQueue -- deduplication, timeout, queue size, flush.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { RequestQueue } from '../src/queue';
import type { QueueOptions, QueueStats } from '../src/queue';

// Suppress PromiseRejectionHandledWarning from fake-timer-based tests
// where the rejection is processed before the test can attach .catch()
const originalListeners = process.listeners('unhandledRejection').length;
process.on('unhandledRejection', () => {});

describe('RequestQueue', () => {
  let queue: RequestQueue;

  beforeEach(() => {
    vi.useFakeTimers();
    queue = new RequestQueue();
  });

  afterEach(async () => {
    await vi.advanceTimersByTimeAsync(100000); // drain all pending timers
    vi.useRealTimers();
  });

  // ── Basic enqueue/execute ─────────────────────────────────────────

  describe('basic enqueue/execute', () => {
    it('executes a function and returns its result', async () => {
      const result = queue.enqueue('key-1', () => Promise.resolve('hello'));

      // Advance timers to allow the promise to settle
      await vi.advanceTimersByTimeAsync(0);
      await expect(result).resolves.toBe('hello');
    });

    it('tracks stats after execution', async () => {
      await queue.enqueue('key-1', () => Promise.resolve('ok'));
      await vi.advanceTimersByTimeAsync(0);

      const stats = queue.stats();
      expect(stats.completed).toBe(1);
      expect(stats.pending).toBe(0);
      expect(stats.deduplicated).toBe(0);
    });

    it('handles async functions', async () => {
      const fn = vi.fn().mockResolvedValue({ data: 42 });

      const result = queue.enqueue('key-1', fn);
      await vi.advanceTimersByTimeAsync(0);
      await expect(result).resolves.toEqual({ data: 42 });
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('propagates errors from the function', async () => {
      const result = queue.enqueue('key-1', () =>
        Promise.reject(new Error('fetch failed')),
      );

      // Use a no-op catch to suppress unhandled rejection, then use rejects
      result.catch(() => {});
      await vi.advanceTimersByTimeAsync(0);
      await expect(result).rejects.toThrow('fetch failed');
    });
  });

  // ── Request deduplication ─────────────────────────────────────────

  describe('request deduplication', () => {
    it('same key returns the same promise (deduplication)', async () => {
      let callCount = 0;
      const fn = () => {
        callCount++;
        return new Promise((resolve) => {
          setTimeout(() => resolve('result'), 100);
        });
      };

      const p1 = queue.enqueue('same-key', fn);
      const p2 = queue.enqueue('same-key', fn);
      const p3 = queue.enqueue('same-key', fn);

      // All promises should be the same reference
      expect(p1).toBe(p2);
      expect(p2).toBe(p3);

      // Only one actual call should be made
      expect(callCount).toBe(1);

      // Advance past the 100ms delay
      await vi.advanceTimersByTimeAsync(150);
      await expect(p1).resolves.toBe('result');
      await expect(p2).resolves.toBe('result');
      await expect(p3).resolves.toBe('result');
    });

    it('different keys execute independently', async () => {
      const fn = (val: string) => Promise.resolve(val);

      const p1 = queue.enqueue('key-a', () => fn('a'));
      const p2 = queue.enqueue('key-b', () => fn('b'));

      await vi.advanceTimersByTimeAsync(0);

      await expect(p1).resolves.toBe('a');
      await expect(p2).resolves.toBe('b');
    });

    it('tracks deduplication count in stats', async () => {
      const fn = () => new Promise<string>((resolve) => {
        setTimeout(() => resolve('ok'), 100);
      });

      queue.enqueue('dup', fn);
      queue.enqueue('dup', fn); // deduplicated
      queue.enqueue('dup', fn); // deduplicated

      expect(queue.stats().deduplicated).toBe(2);

      await vi.advanceTimersByTimeAsync(150);
      await vi.advanceTimersByTimeAsync(0);
      expect(queue.stats().completed).toBe(1);
    });
  });

  // ── Deduplication window expiry ───────────────────────────────────

  describe('deduplication window expiry', () => {
    it('allows re-execution after the original completes', async () => {
      const callCount = { value: 0 };
      const fn = () => {
        callCount.value++;
        return Promise.resolve('result');
      };

      const p1 = queue.enqueue('key-1', fn);
      await vi.advanceTimersByTimeAsync(0);
      await expect(p1).resolves.toBe('result');
      expect(callCount.value).toBe(1);

      // After completion, the same key should create a new request
      const p2 = queue.enqueue('key-1', fn);
      await vi.advanceTimersByTimeAsync(0);
      await expect(p2).resolves.toBe('result');
      expect(callCount.value).toBe(2);
    });
  });

  // ── Timeout ───────────────────────────────────────────────────────

  describe('timeout', () => {
    it('rejects with timeout error after timeoutMs', async () => {
      queue = new RequestQueue({ timeoutMs: 1000 });

      const result = queue.enqueue('slow-key', () =>
        new Promise((resolve) => {
          // Never resolves
          setTimeout(() => resolve('done'), 5000);
        }),
      );

      // Suppress unhandled rejection
      result.catch(() => {});

      // Wait for timeout
      await vi.advanceTimersByTimeAsync(1100);

      await expect(result).rejects.toThrow('Request timed out after 1000ms');
    });

    it('resolves before timeout if function completes quickly', async () => {
      queue = new RequestQueue({ timeoutMs: 5000 });

      const result = queue.enqueue('fast-key', () =>
        Promise.resolve('quick'),
      );

      await vi.advanceTimersByTimeAsync(0);
      await expect(result).resolves.toBe('quick');
    });

    it('per-request timeout override works', async () => {
      queue = new RequestQueue({ timeoutMs: 30000 }); // Long default

      let resolved = false;
      const result = queue.enqueue(
        'override-key',
        () => new Promise((resolve) => {
          setTimeout(() => {
            resolved = true;
            resolve('done');
          }, 5000);
        }),
        100, // Short override
      );

      // Suppress unhandled rejection
      result.catch(() => {});

      // Advance just past the 100ms timeout
      await vi.advanceTimersByTimeAsync(150);

      await expect(result).rejects.toThrow('Request timed out after 100ms');

      // Drain all remaining timers so the inner setTimeout fires cleanly
      // and the settled flag prevents a double-reject
      await vi.advanceTimersByTimeAsync(5000);
      expect(resolved).toBe(true);
    });
  });

  // ── Queue size limit ──────────────────────────────────────────────

  describe('queue size limit', () => {
    it('rejects when queue is full', async () => {
      queue = new RequestQueue({ maxQueueSize: 2 });

      const fn = () => new Promise((resolve) => {
        setTimeout(() => resolve('done'), 1000);
      });

      const p1 = queue.enqueue('key-1', fn);
      const p2 = queue.enqueue('key-2', fn);

      // Third should be rejected
      const p3 = queue.enqueue('key-3', fn);
      await expect(p3).rejects.toThrow('Request queue is full');

      // But first two should still be pending
      expect(queue.stats().pending).toBe(2);

      // Clean up
      await vi.advanceTimersByTimeAsync(1100);
      await expect(p1).resolves.toBe('done');
      await expect(p2).resolves.toBe('done');
    });

    it('allows new requests after pending ones complete', async () => {
      queue = new RequestQueue({ maxQueueSize: 1 });

      const fn = () => new Promise((resolve) => {
        setTimeout(() => resolve('done'), 100);
      });

      const p1 = queue.enqueue('key-1', fn);
      await vi.advanceTimersByTimeAsync(150);
      await expect(p1).resolves.toBe('done');

      // Now queue should be empty, new request should work
      const p2 = queue.enqueue('key-2', fn);
      await vi.advanceTimersByTimeAsync(150);
      await expect(p2).resolves.toBe('done');
    });
  });

  // ── Flush ─────────────────────────────────────────────────────────

  describe('flush', () => {
    it('rejects all pending requests', async () => {
      const p1 = queue.enqueue('key-1', () =>
        new Promise((resolve) => setTimeout(() => resolve('a'), 5000)),
      );
      const p2 = queue.enqueue('key-2', () =>
        new Promise((resolve) => setTimeout(() => resolve('b'), 5000)),
      );

      // Suppress unhandled rejections
      p1.catch(() => {});
      p2.catch(() => {});

      queue.flush();

      await expect(p1).rejects.toThrow('Queue flushed');
      await expect(p2).rejects.toThrow('Queue flushed');

      expect(queue.stats().pending).toBe(0);
    });

    it('flush on empty queue is a no-op', () => {
      expect(() => queue.flush()).not.toThrow();
      expect(queue.stats()).toEqual({
        pending: 0,
        completed: 0,
        deduplicated: 0,
      });
    });
  });
});
