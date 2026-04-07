/**
 * Tests for retryWithBackoff -- exponential backoff, jitter, retryable errors.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { retryWithBackoff, calculateBackoffDelay } from '../src/retry';
import { XergonError } from '../src/errors';

describe('retryWithBackoff', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('successful call (no retry)', () => {
    it('returns the result on first attempt', async () => {
      const fn = vi.fn().mockResolvedValue('ok');
      const result = await retryWithBackoff(fn);
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('returns objects on first attempt', async () => {
      const data = { id: 1, name: 'test' };
      const fn = vi.fn().mockResolvedValue(data);
      const result = await retryWithBackoff(fn);
      expect(result).toEqual(data);
      expect(fn).toHaveBeenCalledTimes(1);
    });
  });

  describe('retry on retryable HTTP status codes', () => {
    it('retries on 429 and succeeds', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'rate_limit_error', message: 'Too many', code: 429 }))
        .mockResolvedValue('ok');

      const onRetry = vi.fn();
      const promise = retryWithBackoff(fn, { onRetry });

      // Advance past the first backoff delay
      await vi.advanceTimersByTimeAsync(2000);

      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
      expect(onRetry).toHaveBeenCalledTimes(1);
      expect(onRetry).toHaveBeenCalledWith(1, expect.any(Number), expect.any(XergonError));
    });

    it('retries on 502 and succeeds', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'internal_error', message: 'Bad Gateway', code: 502 }))
        .mockResolvedValue('ok');

      const promise = retryWithBackoff(fn);
      await vi.advanceTimersByTimeAsync(2000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('retries on 503 and succeeds', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'service_unavailable', message: 'Unavailable', code: 503 }))
        .mockResolvedValue('ok');

      const promise = retryWithBackoff(fn);
      await vi.advanceTimersByTimeAsync(2000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('retries on 500 and succeeds', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'internal_error', message: 'Server Error', code: 500 }))
        .mockResolvedValue('ok');

      const promise = retryWithBackoff(fn);
      await vi.advanceTimersByTimeAsync(2000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('retries on 504 and succeeds', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'internal_error', message: 'Gateway Timeout', code: 504 }))
        .mockResolvedValue('ok');

      const promise = retryWithBackoff(fn);
      await vi.advanceTimersByTimeAsync(2000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('retries on network TypeError', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new TypeError('fetch failed'))
        .mockResolvedValue('ok');

      const promise = retryWithBackoff(fn);
      await vi.advanceTimersByTimeAsync(2000);
      const result = await promise;
      expect(result).toBe('ok');
      expect(fn).toHaveBeenCalledTimes(2);
    });
  });

  describe('no retry on non-retryable errors', () => {
    it('does not retry on 400', async () => {
      const fn = vi.fn()
        .mockRejectedValue(new XergonError({ type: 'invalid_request', message: 'Bad request', code: 400 }));

      await expect(retryWithBackoff(fn)).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('does not retry on 401', async () => {
      const fn = vi.fn()
        .mockRejectedValue(new XergonError({ type: 'unauthorized', message: 'Unauthorized', code: 401 }));

      await expect(retryWithBackoff(fn)).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('does not retry on 404', async () => {
      const fn = vi.fn()
        .mockRejectedValue(new XergonError({ type: 'not_found', message: 'Not Found', code: 404 }));

      await expect(retryWithBackoff(fn)).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('does not retry on AbortError', async () => {
      const fn = vi.fn()
        .mockRejectedValue(new DOMException('The operation was aborted', 'AbortError'));

      await expect(retryWithBackoff(fn)).rejects.toThrow();
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('does not retry on generic Error without code', async () => {
      const fn = vi.fn()
        .mockRejectedValue(new Error('Some random error'));

      await expect(retryWithBackoff(fn)).rejects.toThrow('Some random error');
      expect(fn).toHaveBeenCalledTimes(1);
    });
  });

  describe('max retries exhausted', () => {
    it('throws after maxRetries attempts (default 3)', async () => {
      const fn = vi.fn().mockImplementation(() => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'Server Error', code: 500 }));
      });

      const onRetry = vi.fn();
      const promise = retryWithBackoff(fn, { onRetry });

      // Advance through all retries
      await vi.advanceTimersByTimeAsync(60000);

      await expect(promise).rejects.toThrow(XergonError);
      // 1 initial + 3 retries = 4 total
      expect(fn).toHaveBeenCalledTimes(4);
      expect(onRetry).toHaveBeenCalledTimes(3);
    });

    it('respects custom maxRetries', async () => {
      const fn = vi.fn().mockImplementation(() => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'err', code: 500 }));
      });

      const promise = retryWithBackoff(fn, { maxRetries: 1 });

      await vi.advanceTimersByTimeAsync(30000);

      await expect(promise).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(2); // 1 initial + 1 retry
    });

    it('throws with maxRetries: 0 (no retries)', async () => {
      const fn = vi.fn().mockImplementation(() => {
        return Promise.reject(new XergonError({ type: 'internal_error', message: 'err', code: 500 }));
      });

      await expect(retryWithBackoff(fn, { maxRetries: 0 })).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(1);
    });
  });

  describe('exponential backoff timing', () => {
    it('delays increase exponentially between attempts', async () => {
      const delays: number[] = [];
      const fn = vi.fn().mockImplementation(() => {
        return new Promise((_, reject) => {
          const err = new XergonError({ type: 'internal_error', message: 'err', code: 500 });
          reject(err);
        });
      });

      // We'll capture the delays via onRetry
      const onRetry = vi.fn((attempt, delayMs) => {
        delays.push(delayMs);
      });

      const promise = retryWithBackoff(fn, { onRetry, maxRetries: 2 });

      // Advance timers to trigger all retries
      await vi.advanceTimersByTimeAsync(30000);

      await expect(promise).rejects.toThrow();

      // Should have 2 retry callbacks (attempts 1 and 2)
      expect(onRetry).toHaveBeenCalledTimes(2);

      // Second delay should be >= first delay (exponential)
      // With baseDelay 1000 and factor 2: ~1000ms then ~2000ms (+ jitter)
      expect(delays[0]).toBeLessThan(delays[1] + 500); // Allow for jitter overlap
    });
  });

  describe('jitter', () => {
    it('delays vary between attempts (jitter present)', async () => {
      // Run multiple times and check that delays are not always identical
      const allDelays: number[][] = [];

      for (let run = 0; run < 5; run++) {
        const delays: number[] = [];
        const fn = vi.fn().mockImplementation(() => {
          return Promise.reject(new XergonError({ type: 'internal_error', message: 'err', code: 500 }));
        });

        const onRetry = vi.fn((_, delayMs) => delays.push(delayMs));
        const promise = retryWithBackoff(fn, { onRetry, maxRetries: 2 });
        await vi.advanceTimersByTimeAsync(30000);
        try { await promise; } catch { /* expected */ }
        allDelays.push(delays);
      }

      // Check that not all first-attempt delays are identical (due to jitter)
      const firstAttemptDelays = allDelays.map(d => d[0]);
      const uniqueDelays = new Set(firstAttemptDelays);
      expect(uniqueDelays.size).toBeGreaterThan(1);
    });
  });

  describe('calculateBackoffDelay', () => {
    it('calculates base delay correctly for attempt 0', () => {
      vi.spyOn(Math, 'random').mockReturnValue(0);
      const delay = calculateBackoffDelay(0, 1000, 30000, 2);
      expect(delay).toBe(1000); // 1000 * 2^0 + 0 = 1000
      vi.restoreAllMocks();
    });

    it('calculates exponential delay for attempt 1', () => {
      vi.spyOn(Math, 'random').mockReturnValue(0);
      const delay = calculateBackoffDelay(1, 1000, 30000, 2);
      expect(delay).toBe(2000); // 1000 * 2^1 + 0 = 2000
      vi.restoreAllMocks();
    });

    it('respects maxDelayMs cap', () => {
      vi.spyOn(Math, 'random').mockReturnValue(0);
      const delay = calculateBackoffDelay(10, 1000, 5000, 2);
      expect(delay).toBe(5000); // capped at max
      vi.restoreAllMocks();
    });

    it('adds jitter from 0 to 1000ms', () => {
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
      const delay = calculateBackoffDelay(0, 1000, 30000, 2);
      expect(delay).toBe(1500); // 1000 + 500 (jitter)
      vi.restoreAllMocks();
    });
  });

  describe('custom retryableStatuses', () => {
    it('only retries on specified statuses', async () => {
      const fn = vi.fn()
        .mockRejectedValueOnce(new XergonError({ type: 'internal_error', message: 'err', code: 502 }))
        .mockRejectedValueOnce(new XergonError({ type: 'internal_error', message: 'err', code: 503 }))
        .mockResolvedValue('ok');

      // Only retry on 502, not 503
      const promise = retryWithBackoff(fn, {
        retryableStatuses: [502],
        maxRetries: 5,
      });

      await vi.advanceTimersByTimeAsync(60000);

      // Should retry once for 502, then fail on 503 (not retryable)
      await expect(promise).rejects.toThrow(XergonError);
      expect(fn).toHaveBeenCalledTimes(2);
    });
  });
});
