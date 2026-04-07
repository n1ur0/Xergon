/**
 * Tests for RetryClient -- class-based retry with multiple backoff strategies.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { RetryClient } from '../src/retry';
import type { BackoffStrategy } from '../src/retry';
import { XergonError } from '../src/errors';

describe('RetryClient', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('exponential backoff', () => {
    it('succeeds without retry on first attempt', async () => {
      const client = new RetryClient();
      const fn = vi.fn((_signal: AbortSignal) => Promise.resolve('ok'));
      const result = await client.execute(fn);
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('retries with exponential backoff and succeeds', async () => {
      const client = new RetryClient({ jitter: false });
      const fn = vi.fn()
        .mockImplementationOnce((_signal: AbortSignal) => {
          const err = new XergonError({ type: 'internal_error', message: 'fail', code: 500 });
          return Promise.reject(err);
        })
        .mockImplementationOnce((_signal: AbortSignal) => Promise.resolve('ok'));

      const promise = client.execute(fn);
      await vi.advanceTimersByTimeAsync(5000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('delays increase exponentially', async () => {
      const delays: number[] = [];
      const client = new RetryClient({
        jitter: false,
        onRetry: (_attempt, _err, delayMs) => delays.push(delayMs),
      });

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn, { strategy: 'exponential' });
      await vi.advanceTimersByTimeAsync(60000);
      try { await promise; } catch { /* expected */ }

      // With baseDelayMs=1000, multiplier=2: attempt 0 -> 1000ms, attempt 1 -> 2000ms
      expect(delays).toHaveLength(3);
      expect(delays[0]).toBe(1000); // 1000 * 2^0
      expect(delays[1]).toBe(2000); // 1000 * 2^1
      expect(delays[2]).toBe(4000); // 1000 * 2^2
    });
  });

  describe('linear backoff', () => {
    it('delays increase linearly', async () => {
      const delays: number[] = [];
      const client = new RetryClient({
        jitter: false,
        onRetry: (_attempt, _err, delayMs) => delays.push(delayMs),
      });

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn, { strategy: 'linear' });
      await vi.advanceTimersByTimeAsync(60000);
      try { await promise; } catch { /* expected */ }

      // Linear: baseDelay + attempt * baseDelay => 1000, 2000, 3000
      expect(delays).toHaveLength(3);
      expect(delays[0]).toBe(1000);
      expect(delays[1]).toBe(2000);
      expect(delays[2]).toBe(3000);
    });
  });

  describe('constant backoff', () => {
    it('delays stay constant', async () => {
      const delays: number[] = [];
      const client = new RetryClient({
        jitter: false,
        onRetry: (_attempt, _err, delayMs) => delays.push(delayMs),
      });

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn, { strategy: 'constant' });
      await vi.advanceTimersByTimeAsync(60000);
      try { await promise; } catch { /* expected */ }

      expect(delays).toHaveLength(3);
      expect(delays[0]).toBe(1000);
      expect(delays[1]).toBe(1000);
      expect(delays[2]).toBe(1000);
    });
  });

  describe('jitter', () => {
    it('adds random jitter to delays', async () => {
      const allDelays: number[][] = [];

      for (let run = 0; run < 5; run++) {
        const delays: number[] = [];
        const client = new RetryClient({
          jitter: true,
          jitterRangeMs: 500,
          onRetry: (_attempt, _err, delayMs) => delays.push(delayMs),
        });

        const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
          return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
        });

        const promise = client.execute(fn);
        await vi.advanceTimersByTimeAsync(60000);
        try { await promise; } catch { /* expected */ }
        allDelays.push(delays);
      }

      // First attempt delays should vary due to jitter
      const firstDelays = allDelays.map(d => d[0]);
      const uniqueDelays = new Set(firstDelays);
      expect(uniqueDelays.size).toBeGreaterThan(1);
    });

    it('jitter can be disabled', async () => {
      const client = new RetryClient({ jitter: false });
      const delay = client.calculateDelay(0, 'exponential');
      expect(delay).toBe(1000); // baseDelayMs * 2^0 = 1000, no jitter
    });
  });

  describe('max retries exhausted', () => {
    it('throws after maxRetries (default 3)', async () => {
      const client = new RetryClient({ jitter: false });
      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn);
      await vi.advanceTimersByTimeAsync(60000);
      await expect(promise).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(4); // 1 initial + 3 retries
    });

    it('respects custom maxRetries', async () => {
      const client = new RetryClient({ maxRetries: 1, jitter: false });
      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn);
      await vi.advanceTimersByTimeAsync(30000);
      await expect(promise).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(2); // 1 initial + 1 retry
    });
  });

  describe('isRetryable()', () => {
    it('returns true for retryable HTTP status codes', () => {
      const client = new RetryClient();
      expect(client.isRetryable(new XergonError({ type: 'internal_error', message: '', code: 408 }))).toBe(true);
      expect(client.isRetryable(new XergonError({ type: 'rate_limit_error', message: '', code: 429 }))).toBe(true);
      expect(client.isRetryable(new XergonError({ type: 'internal_error', message: '', code: 500 }))).toBe(true);
      expect(client.isRetryable(new XergonError({ type: 'internal_error', message: '', code: 502 }))).toBe(true);
      expect(client.isRetryable(new XergonError({ type: 'service_unavailable', message: '', code: 503 }))).toBe(true);
      expect(client.isRetryable(new XergonError({ type: 'internal_error', message: '', code: 504 }))).toBe(true);
    });

    it('returns false for non-retryable status codes', () => {
      const client = new RetryClient();
      expect(client.isRetryable(new XergonError({ type: 'invalid_request', message: '', code: 400 }))).toBe(false);
      expect(client.isRetryable(new XergonError({ type: 'unauthorized', message: '', code: 401 }))).toBe(false);
      expect(client.isRetryable(new XergonError({ type: 'not_found', message: '', code: 404 }))).toBe(false);
    });

    it('returns true for retryable error codes', () => {
      const client = new RetryClient();
      const err1 = new Error('connection refused');
      Object.assign(err1, { code: 'ECONNREFUSED' });
      expect(client.isRetryable(err1)).toBe(true);

      const err2 = new Error('connection reset');
      Object.assign(err2, { code: 'ECONNRESET' });
      expect(client.isRetryable(err2)).toBe(true);

      const err3 = new Error('timed out');
      Object.assign(err3, { code: 'ETIMEDOUT' });
      expect(client.isRetryable(err3)).toBe(true);

      const err4 = new Error('not found');
      Object.assign(err4, { code: 'ENOTFOUND' });
      expect(client.isRetryable(err4)).toBe(true);
    });

    it('returns false for AbortError', () => {
      const client = new RetryClient();
      const err = new DOMException('Aborted', 'AbortError');
      expect(client.isRetryable(err)).toBe(false);
    });

    it('returns true for network TypeErrors', () => {
      const client = new RetryClient();
      expect(client.isRetryable(new TypeError('fetch failed'))).toBe(true);
      expect(client.isRetryable(new TypeError('network error'))).toBe(true);
    });
  });

  describe('abort signal', () => {
    it('throws immediately if signal is already aborted', async () => {
      const client = new RetryClient();
      const controller = new AbortController();
      controller.abort();

      const fn = vi.fn();
      await expect(client.execute(fn, { signal: controller.signal })).rejects.toThrow('aborted');
      expect(fn).not.toHaveBeenCalled();
    });

    it('throws AbortError when signal aborts between retries', async () => {
      const client = new RetryClient({ jitter: false });
      const controller = new AbortController();

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'fail', code: 500 }));
      });

      const promise = client.execute(fn, { signal: controller.signal });

      // Let the first retry delay start, then abort
      await vi.advanceTimersByTimeAsync(500);
      controller.abort('user cancelled');

      await expect(promise).rejects.toThrow();
    });
  });

  describe('retry callback', () => {
    it('calls onRetry for each retry', async () => {
      const onRetry = vi.fn();
      const client = new RetryClient({ jitter: false, onRetry });

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        const err = new XergonError({ type: 'internal_error', message: 'fail', code: 500 });
        return Promise.reject(err);
      });

      const promise = client.execute(fn);
      // Catch to prevent unhandled rejection during timer advancement
      promise.catch(() => {});
      await vi.advanceTimersByTimeAsync(60000);

      expect(onRetry).toHaveBeenCalledTimes(3);
      expect(onRetry).toHaveBeenCalledWith(1, expect.any(Error), 1000);
      expect(onRetry).toHaveBeenCalledWith(2, expect.any(Error), 2000);
      expect(onRetry).toHaveBeenCalledWith(3, expect.any(Error), 4000);
    });
  });

  describe('stats tracking', () => {
    it('tracks total retries and failures', async () => {
      const client = new RetryClient({ jitter: false });

      let rejectCount = 0;
      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        rejectCount++;
        const err = new XergonError({ type: 'internal_error', message: 'fail', code: 500 });
        return Promise.reject(err);
      });

      const promise = client.execute(fn);
      // Catch the promise to avoid unhandled rejection during timer advancement
      promise.catch(() => {});
      await vi.advanceTimersByTimeAsync(60000);

      const stats = client.getStats();
      expect(stats.totalRetries).toBe(3);
      expect(stats.totalFailures).toBe(1);
      expect(stats.lastError).toBeDefined();
    });

    it('resets stats', async () => {
      const client = new RetryClient({ jitter: false, maxRetries: 0 });

      const fn = vi.fn().mockImplementation((_signal: AbortSignal) => {
        const err = new XergonError({ type: 'internal_error', message: 'fail', code: 500 });
        return Promise.reject(err);
      });

      try { await client.execute(fn); } catch { /* expected */ }

      expect(client.getStats().totalFailures).toBe(1);
      client.resetStats();
      expect(client.getStats().totalRetries).toBe(0);
      expect(client.getStats().totalFailures).toBe(0);
      expect(client.getStats().lastError).toBeUndefined();
    });
  });

  describe('calculateDelay()', () => {
    it('respects maxDelayMs cap', () => {
      const client = new RetryClient({ jitter: false, maxDelayMs: 5000 });
      // exponential at attempt 10: 1000 * 2^10 = 1024000, capped at 5000
      expect(client.calculateDelay(10, 'exponential')).toBe(5000);
    });

    it('linear backoff caps at maxDelayMs', () => {
      const client = new RetryClient({ jitter: false, maxDelayMs: 3000 });
      // linear at attempt 5: 1000 + 5*1000 = 6000, capped at 3000
      expect(client.calculateDelay(5, 'linear')).toBe(3000);
    });
  });

  describe('passes AbortSignal to fn', () => {
    it('passes a per-attempt AbortSignal', async () => {
      const client = new RetryClient();
      const receivedSignals: AbortSignal[] = [];

      const fn = vi.fn().mockImplementation((signal: AbortSignal) => {
        receivedSignals.push(signal);
        return Promise.resolve('ok');
      });

      await client.execute(fn);
      expect(fn).toHaveBeenCalledTimes(1);
      expect(receivedSignals).toHaveLength(1);
      expect(receivedSignals[0]).toBeInstanceOf(AbortSignal);
    });
  });
});
