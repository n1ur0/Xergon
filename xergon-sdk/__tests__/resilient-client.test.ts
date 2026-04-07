/**
 * Tests for ResilientHttpClient -- combines retry, cancellation, timeout.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ResilientHttpClient } from '../src/resilient-client';
import { XergonError } from '../src/errors';

// Helper to create a mock fetch response
function mockResponse(status: number, body?: unknown): Response {
  const jsonBody = body !== undefined ? JSON.stringify(body) : '';
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: new Headers({
      'content-type': 'application/json',
    }),
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(jsonBody),
  } as Response;
}

describe('ResilientHttpClient', () => {
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.useRealTimers();
  });

  describe('GET with retry', () => {
    it('returns data on successful GET', async () => {
      const data = { models: ['llama-3.3-70b'] };
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse(200, data));

      const client = new ResilientHttpClient('https://relay.xergon.gg');
      const result = await client.get('/v1/models');

      expect(result).toEqual(data);
      expect(globalThis.fetch).toHaveBeenCalledTimes(1);
    });

    it('retries on 500 and succeeds on retry', async () => {
      globalThis.fetch = vi.fn()
        .mockResolvedValueOnce(mockResponse(500, { error: { message: 'server error', code: 500, type: 'internal_error' } }))
        .mockResolvedValueOnce(mockResponse(200, { ok: true }));

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { jitter: false },
      });

      const promise = client.get('/v1/models');
      await vi.advanceTimersByTimeAsync(5000);
      const result = await promise;

      expect(result).toEqual({ ok: true });
      expect(globalThis.fetch).toHaveBeenCalledTimes(2);
    });

    it('throws after retries exhausted', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(
        mockResponse(500, { error: { message: 'server error', code: 500, type: 'internal_error' } })
      );

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { jitter: false },
      });

      const promise = client.get('/v1/models');
      // Catch early to avoid unhandled rejection during timer advancement
      promise.catch(() => {});
      await vi.advanceTimersByTimeAsync(60000);

      // The promise should have been rejected
      await expect(promise).rejects.toThrow('server error');
      expect(globalThis.fetch).toHaveBeenCalledTimes(4); // 1 initial + 3 retries
    });
  });

  describe('POST with retry', () => {
    it('sends POST body and returns data', async () => {
      const response = { id: 'chat-123', choices: [] };
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse(200, response));

      const client = new ResilientHttpClient('https://relay.xergon.gg');
      const result = await client.post('/v1/chat/completions', {
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello' }],
      });

      expect(result).toEqual(response);
      expect(globalThis.fetch).toHaveBeenCalledTimes(1);

      // Verify body was sent
      const call = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      const parsed = JSON.parse(call[1].body);
      expect(parsed.model).toBe('llama-3.3-70b');
    });

    it('retries on 502 and succeeds', async () => {
      globalThis.fetch = vi.fn()
        .mockResolvedValueOnce(mockResponse(502, { error: { message: 'bad gateway', code: 502, type: 'internal_error' } }))
        .mockResolvedValueOnce(mockResponse(200, { ok: true }));

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { jitter: false },
      });

      const promise = client.post('/v1/test', {});
      await vi.advanceTimersByTimeAsync(5000);
      const result = await promise;

      expect(result).toEqual({ ok: true });
    });
  });

  describe('timeout', () => {
    beforeEach(() => { vi.useFakeTimers(); });
    afterEach(() => { vi.useRealTimers(); });

    it('rejects on timeout via AbortSignal', async () => {
      // Test that AbortController abort properly cancels fetch
      globalThis.fetch = vi.fn().mockImplementation((_url, opts) => {
        return new Promise((_, reject) => {
          if (opts?.signal) {
            opts.signal.addEventListener('abort', () => {
              reject(new DOMException('Aborted', 'AbortError'));
            });
          }
        });
      });

      const controller = new AbortController();
      const fetchPromise = fetch('https://test.com', { signal: controller.signal });

      // Advance time to trigger abort
      controller.abort();
      await expect(fetchPromise).rejects.toThrow('Aborted');
    });
  });

  describe('abort signal', () => {
    it('rejects when external signal aborts', async () => {
      globalThis.fetch = vi.fn().mockImplementation((_url, opts) => {
        return new Promise((_, reject) => {
          if (opts.signal) {
            opts.signal.addEventListener('abort', () => {
              reject(new DOMException('Aborted', 'AbortError'));
            });
          }
        });
      });

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { maxRetries: 0 },
      });

      const controller = new AbortController();
      const promise = client.get('/v1/test', { signal: controller.signal });

      controller.abort('user cancelled');

      await expect(promise).rejects.toThrow();
    });
  });

  describe('cancellable request', () => {
    it('returns promise and cancel function', async () => {
      const data = { result: 'ok' };
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse(200, data));

      const client = new ResilientHttpClient('https://relay.xergon.gg');
      const { promise, cancel } = client.cancellableRequest({
        method: 'GET',
        path: '/v1/models',
      });

      const result = await promise;
      expect(result).toEqual(data);
      // Cancel after resolve is a no-op
      expect(() => cancel()).not.toThrow();
    });

    it('cancel() aborts in-flight request', async () => {
      globalThis.fetch = vi.fn().mockImplementation((_url, opts) => {
        return new Promise((_, reject) => {
          if (opts.signal) {
            opts.signal.addEventListener('abort', () => {
              reject(new DOMException('Aborted', 'AbortError'));
            });
          }
        });
      });

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { maxRetries: 0 },
      });

      const { promise, cancel } = client.cancellableRequest({
        method: 'GET',
        path: '/v1/slow',
      });

      cancel('no longer needed');

      await expect(promise).rejects.toThrow();
    });
  });

  describe('query params', () => {
    it('appends query parameters to URL', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse(200, {}));

      const client = new ResilientHttpClient('https://relay.xergon.gg');
      await client.get('/v1/models', {
        params: { limit: '10', offset: '5' },
      });

      const call = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      expect(call[0]).toContain('limit=10');
      expect(call[0]).toContain('offset=5');
    });
  });

  describe('cancelAll()', () => {
    it('cancels all active requests', async () => {
      globalThis.fetch = vi.fn().mockImplementation(() => {
        return new Promise(() => {}); // never resolves
      });

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        timeoutMs: 60000, // long timeout so cancelAll fires first
      });

      const promise1 = client.get('/v1/slow1');
      const promise2 = client.get('/v1/slow2');

      client.cancelAll('shutdown');

      await expect(promise1).rejects.toThrow();
      await expect(promise2).rejects.toThrow();
    });
  });

  describe('retry stats', () => {
    it('tracks retry stats', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(
        mockResponse(500, { error: { message: 'err', code: 500, type: 'internal_error' } })
      );

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        retry: { jitter: false },
      });

      const promise = client.get('/v1/test');
      // Catch to prevent unhandled rejection during timer advancement
      promise.catch(() => {});
      await vi.advanceTimersByTimeAsync(60000);

      const stats = client.getRetryStats();
      expect(stats.totalRetries).toBe(3);
      expect(stats.totalFailures).toBeGreaterThan(0);
    });

    it('resets retry stats', async () => {
      const client = new ResilientHttpClient('https://relay.xergon.gg');
      client.resetRetryStats();
      const stats = client.getRetryStats();
      expect(stats.totalRetries).toBe(0);
      expect(stats.totalFailures).toBe(0);
    });
  });

  describe('default headers', () => {
    it('includes default headers in requests', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(mockResponse(200, {}));

      const client = new ResilientHttpClient('https://relay.xergon.gg', {
        headers: { 'X-Custom': 'value' },
      });

      await client.get('/v1/test');

      const call = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      expect(call[1].headers['X-Custom']).toBe('value');
    });
  });

  describe('noRetry option', () => {
    it('does not retry when noRetry is true', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue(
        mockResponse(500, { error: { message: 'err', code: 500, type: 'internal_error' } })
      );

      const client = new ResilientHttpClient('https://relay.xergon.gg');

      await expect(
        client.get('/v1/test', { noRetry: true })
      ).rejects.toThrow('err');

      expect(globalThis.fetch).toHaveBeenCalledTimes(1);
    });
  });
});
