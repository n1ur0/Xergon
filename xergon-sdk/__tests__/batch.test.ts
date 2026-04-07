/**
 * Tests for BatchClient -- parallel/sequential batch execution, error isolation, concurrency.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { BatchClient } from '../src/batch';
import type { BatchRequestItem, BatchResponseItem } from '../src/batch';

// ── Helpers ──────────────────────────────────────────────────────────

function mockFetchOk(
  data: unknown,
  url?: string,
  delay = 0,
): ReturnType<typeof fetch> {
  const start = Date.now();
  return new Promise((resolve) => {
    setTimeout(() => {
      resolve({
        ok: true,
        status: 200,
        headers: new Headers({
          'content-type': 'application/json',
          'x-request-id': 'test-123',
        }),
        json: () => Promise.resolve(data),
        text: () => Promise.resolve(JSON.stringify(data)),
        body: null,
      } as Response);
    }, delay);
  });
}

function mockFetchError(status: number, message: string, delay = 0): ReturnType<typeof fetch> {
  return new Promise((resolve) => {
    setTimeout(() => {
      resolve({
        ok: false,
        status,
        statusText: message,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () =>
          Promise.resolve({
            error: { type: 'internal_error', message, code: status },
          }),
        text: () => Promise.resolve(message),
        body: null,
      } as Response);
    }, delay);
  });
}

function mockFetchThrow(err: Error, delay = 0): ReturnType<typeof fetch> {
  return new Promise((_, reject) => {
    setTimeout(() => reject(err), delay);
  });
}

// ── Tests ────────────────────────────────────────────────────────────

describe('BatchClient', () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;
  let client: BatchClient;

  beforeEach(() => {
    client = new BatchClient('https://relay.xergon.gg');
    fetchSpy = vi.spyOn(globalThis, 'fetch');
  });

  afterEach(() => {
    fetchSpy.mockRestore();
  });

  // ── Basic parallel execution ──────────────────────────────────────

  describe('parallel execution', () => {
    it('executes multiple requests in parallel and returns responses', async () => {
      fetchSpy.mockImplementation((url: any) => {
        if (url.includes('/models')) {
          return mockFetchOk({ id: 'llama-3.3-70b', object: 'model', ownedBy: 'test' });
        }
        if (url.includes('/chat/completions')) {
          return mockFetchOk({
            id: 'chatcmpl-123',
            object: 'chat.completion',
            created: 1234567890,
            model: 'llama-3.3-70b',
            choices: [{ index: 0, message: { role: 'assistant', content: 'Hello!' }, finish_reason: 'stop' }],
          });
        }
        return mockFetchOk({});
      });

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/models' },
          { id: 'req-2', method: 'POST' as const, path: '/v1/chat/completions', body: { model: 'llama-3.3-70b', messages: [{ role: 'user', content: 'Hi' }] } },
        ],
      };

      const result = await client.execute(batch);

      expect(result.responses).toHaveLength(2);
      expect(result.total_duration_ms).toBeGreaterThanOrEqual(0);

      // Responses are correlated by ID
      const byId = new Map(result.responses.map((r) => [r.id, r]));
      expect(byId.get('req-1')!.status).toBe(200);
      expect(byId.get('req-1')!.body.id).toBe('llama-3.3-70b');
      expect(byId.get('req-2')!.status).toBe(200);
      expect(byId.get('req-2')!.body.choices[0].message.content).toBe('Hello!');
    });

    it('tracks per-request duration', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }, undefined, 10));

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/models' },
          { id: 'req-2', method: 'GET' as const, path: '/v1/models' },
        ],
      };

      const result = await client.execute(batch);

      for (const resp of result.responses) {
        expect(resp.duration_ms).toBeGreaterThanOrEqual(0);
      }
    });

    it('preserves response headers', async () => {
      fetchSpy.mockImplementation(() =>
        mockFetchOk({ ok: true }),
      );

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/models' },
        ],
      };

      const result = await client.execute(batch);
      expect(result.responses[0].headers['content-type']).toBe('application/json');
      expect(result.responses[0].headers['x-request-id']).toBe('test-123');
    });
  });

  // ── Sequential execution ──────────────────────────────────────────

  describe('sequential execution', () => {
    it('executes requests one after another in order', async () => {
      const callOrder: string[] = [];
      fetchSpy.mockImplementation((url: any) => {
        callOrder.push(url);
        return mockFetchOk({ ok: true });
      });

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/models' },
          { id: 'req-2', method: 'GET' as const, path: '/v1/providers' },
          { id: 'req-3', method: 'GET' as const, path: '/v1/health' },
        ],
        sequential: true,
      };

      const result = await client.execute(batch);

      expect(result.responses).toHaveLength(3);
      expect(callOrder).toEqual([
        'https://relay.xergon.gg/v1/models',
        'https://relay.xergon.gg/v1/providers',
        'https://relay.xergon.gg/v1/health',
      ]);
      // Responses should be in order
      expect(result.responses[0].id).toBe('req-1');
      expect(result.responses[1].id).toBe('req-2');
      expect(result.responses[2].id).toBe('req-3');
    });
  });

  // ── Error isolation ───────────────────────────────────────────────

  describe('error isolation', () => {
    it('one failure does not affect other requests', async () => {
      fetchSpy.mockImplementation((url: any) => {
        if (url.includes('/fail')) {
          return mockFetchError(500, 'Internal Server Error');
        }
        return mockFetchOk({ success: true });
      });

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/ok' },
          { id: 'req-2', method: 'GET' as const, path: '/v1/fail' },
          { id: 'req-3', method: 'GET' as const, path: '/v1/ok2' },
        ],
      };

      const result = await client.execute(batch);

      expect(result.responses).toHaveLength(3);
      // Failed request still has a response (non-200 status, body contains error)
      expect(result.responses[1].status).toBe(500);
      expect(result.responses[1].body.error.message).toBe('Internal Server Error');
      // Other requests succeed
      expect(result.responses[0].status).toBe(200);
      expect(result.responses[2].status).toBe(200);
    });

    it('network error returns status 0 with error message', async () => {
      fetchSpy.mockImplementation(() => mockFetchThrow(new Error('Network error')));

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/test' },
        ],
      };

      const result = await client.execute(batch);

      expect(result.responses).toHaveLength(1);
      expect(result.responses[0].status).toBe(0);
      expect(result.responses[0].error).toBe('Network error');
    });

    it('timeout returns status 0 with abort error', async () => {
      fetchSpy.mockImplementation(
        () =>
          new Promise((_, reject) => {
            setTimeout(() => reject(new DOMException('Aborted', 'AbortError')), 1000);
          }),
      );

      const batch = {
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/test' },
        ],
        timeoutMs: 10, // Very short timeout
      };

      const result = await client.execute(batch);

      expect(result.responses).toHaveLength(1);
      expect(result.responses[0].status).toBe(0);
      expect(result.responses[0].error).toBeTruthy();
    });
  });

  // ── Concurrency limit ─────────────────────────────────────────────

  describe('concurrency limit', () => {
    it('respects maxConcurrent', async () => {
      let concurrentCount = 0;
      let maxConcurrent = 0;

      fetchSpy.mockImplementation(() => {
        concurrentCount++;
        maxConcurrent = Math.max(maxConcurrent, concurrentCount);

        return new Promise((resolve) => {
          setTimeout(() => {
            concurrentCount--;
            resolve({
              ok: true,
              status: 200,
              headers: new Headers({ 'content-type': 'application/json' }),
              json: () => Promise.resolve({ ok: true }),
              text: () => Promise.resolve('{}'),
              body: null,
            } as Response);
          }, 50);
        });
      });

      client.maxConcurrent = 2;

      const batch = {
        requests: Array.from({ length: 10 }, (_, i) => ({
          id: `req-${i}`,
          method: 'GET' as const,
          path: `/v1/test/${i}`,
        })),
      };

      await client.execute(batch);

      expect(maxConcurrent).toBeLessThanOrEqual(2);
      expect(maxConcurrent).toBeGreaterThanOrEqual(1);
    });
  });

  // ── Request/response ID correlation ───────────────────────────────

  describe('ID correlation', () => {
    it('response IDs match request IDs exactly', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }));

      const ids = ['alpha', 'beta', 'gamma', 'delta'];
      const batch = {
        requests: ids.map((id) => ({
          id,
          method: 'GET' as const,
          path: `/v1/test/${id}`,
        })),
      };

      const result = await client.execute(batch);

      const responseIds = result.responses.map((r) => r.id);
      expect(responseIds.sort()).toEqual(ids.sort());
    });
  });

  // ── chatBatch convenience ─────────────────────────────────────────

  describe('chatBatch', () => {
    it('creates batch request items for chat completions', async () => {
      fetchSpy.mockImplementation(() =>
        mockFetchOk({
          id: 'chatcmpl-123',
          object: 'chat.completion',
          created: 1234567890,
          model: 'llama-3.3-70b',
          choices: [{ index: 0, message: { role: 'assistant', content: 'Hi!' }, finish_reason: 'stop' }],
        }),
      );

      const result = await client.chatBatch([
        { model: 'llama-3.3-70b', messages: [{ role: 'user', content: 'Hello' }] },
        { model: 'mistral-7b', messages: [{ role: 'user', content: 'World' }] },
      ]);

      expect(result.responses).toHaveLength(2);
      expect(result.responses[0].id).toBe('chat-0');
      expect(result.responses[1].id).toBe('chat-1');

      // Verify the correct URL was called
      expect(fetchSpy).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/chat/completions',
        expect.objectContaining({ method: 'POST' }),
      );
    });

    it('respects custom IDs', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }));

      const result = await client.chatBatch([
        { id: 'my-custom-id', model: 'llama-3.3-70b', messages: [{ role: 'user', content: 'Hello' }] },
      ]);

      expect(result.responses[0].id).toBe('my-custom-id');
    });
  });

  // ── modelInfoBatch convenience ────────────────────────────────────

  describe('modelInfoBatch', () => {
    it('creates GET requests for model info', async () => {
      fetchSpy.mockImplementation((url: any) => {
        const modelId = url.split('/').pop();
        return mockFetchOk({ id: modelId, object: 'model', ownedBy: 'test' });
      });

      const result = await client.modelInfoBatch(['llama-3.3-70b', 'mistral-7b']);

      expect(result.responses).toHaveLength(2);
      expect(result.responses[0].id).toBe('model-0');
      expect(result.responses[1].id).toBe('model-1');
      expect(result.responses[0].body.id).toBe('llama-3.3-70b');
      expect(result.responses[1].body.id).toBe('mistral-7b');

      // Verify GET method was used
      expect(fetchSpy).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/models/llama-3.3-70b',
        expect.objectContaining({ method: 'GET' }),
      );
    });

    it('URL-encodes model IDs with special characters', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }));

      await client.modelInfoBatch(['model/special+chars']);

      expect(fetchSpy).toHaveBeenCalledWith(
        'https://relay.xergon.gg/v1/models/model%2Fspecial%2Bchars',
        expect.any(Object),
      );
    });
  });

  // ── Default headers ───────────────────────────────────────────────

  describe('default headers', () => {
    it('includes default headers on all requests', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }));

      client = new BatchClient('https://relay.xergon.gg', {
        'X-Custom-Header': 'custom-value',
        Authorization: 'Bearer token123',
      });

      await client.execute({
        requests: [
          { id: 'req-1', method: 'GET' as const, path: '/v1/test' },
        ],
      });

      const call = fetchSpy.mock.calls[0];
      const headers = call[1]?.headers as Record<string, string>;
      expect(headers['X-Custom-Header']).toBe('custom-value');
      expect(headers['Authorization']).toBe('Bearer token123');
    });

    it('request-specific headers override defaults', async () => {
      fetchSpy.mockImplementation(() => mockFetchOk({ ok: true }));

      client = new BatchClient('https://relay.xergon.gg', {
        'X-Default': 'default',
      });

      await client.execute({
        requests: [
          {
            id: 'req-1',
            method: 'GET' as const,
            path: '/v1/test',
            headers: { 'X-Default': 'override', 'X-Extra': 'extra' },
          },
        ],
      });

      const call = fetchSpy.mock.calls[0];
      const headers = call[1]?.headers as Record<string, string>;
      expect(headers['X-Default']).toBe('override');
      expect(headers['X-Extra']).toBe('extra');
    });
  });

  // ── Empty batch ───────────────────────────────────────────────────

  describe('empty batch', () => {
    it('returns empty responses for empty batch', async () => {
      const result = await client.execute({ requests: [] });

      expect(result.responses).toHaveLength(0);
      expect(result.total_duration_ms).toBeGreaterThanOrEqual(0);
    });
  });
});
