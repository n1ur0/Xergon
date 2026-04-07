/**
 * Tests for chat completions (streaming and non-streaming).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { XergonClient } from '../src/index';
import { XergonError } from '../src/errors';

describe('Chat Completions', () => {
  const mockCompletionResponse = {
    id: 'chatcmpl-abc123',
    object: 'chat.completion',
    created: 1700000000,
    model: 'llama-3.3-70b',
    choices: [
      {
        index: 0,
        message: { role: 'assistant', content: 'Hello! How can I help you?' },
        finishReason: 'stop',
      },
    ],
    usage: {
      promptTokens: 10,
      completionTokens: 20,
      totalTokens: 30,
    },
  };

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('non-streaming completion', () => {
    it('returns a ChatCompletionResponse on success', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockCompletionResponse),
      }));

      const client = new XergonClient();
      const result = await client.chat.completions.create({
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello!' }],
      });

      expect(result.id).toBe('chatcmpl-abc123');
      expect(result.object).toBe('chat.completion');
      expect(result.model).toBe('llama-3.3-70b');
      expect(result.choices).toHaveLength(1);
      expect(result.choices[0].message.role).toBe('assistant');
      expect(result.choices[0].message.content).toBe('Hello! How can I help you?');
      expect(result.choices[0].finishReason).toBe('stop');
      expect(result.usage).toBeDefined();
      expect(result.usage!.totalTokens).toBe(30);
    });

    it('sends POST to /v1/chat/completions with correct body', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockCompletionResponse),
      }));

      const client = new XergonClient();
      await client.chat.completions.create({
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello!' }],
        maxTokens: 100,
        temperature: 0.7,
      });

      expect(fetch).toHaveBeenCalledTimes(1);
      const call = (fetch as ReturnType<typeof vi.fn>).mock.calls[0];
      const url = call[0] as string;
      const options = call[1] as RequestInit;

      expect(url).toContain('/v1/chat/completions');
      expect(options.method).toBe('POST');
      expect(options.headers).toHaveProperty('Content-Type', 'application/json');

      const body = JSON.parse(options.body as string);
      expect(body.model).toBe('llama-3.3-70b');
      expect(body.messages).toEqual([{ role: 'user', content: 'Hello!' }]);
      expect(body.maxTokens).toBe(100);
      expect(body.temperature).toBe(0.7);
      expect(body.stream).toBe(false);
    });

    it('includes auth headers when public key is set', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve(mockCompletionResponse),
      }));

      const client = new XergonClient({ publicKey: '0xtestpub' });
      await client.chat.completions.create({
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello!' }],
      });

      const options = (fetch as ReturnType<typeof vi.fn>).mock.calls[0][1] as RequestInit;
      expect(options.headers).toHaveProperty('X-Xergon-Public-Key', '0xtestpub');
    });
  });

  describe('error handling', () => {
    it('throws XergonError with type unauthorized on 401', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        statusText: 'Unauthorized',
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve({
          error: {
            type: 'unauthorized',
            message: 'Invalid API key',
            code: 401,
          },
        }),
      }));

      const client = new XergonClient();

      await expect(
        client.chat.completions.create({
          model: 'llama-3.3-70b',
          messages: [{ role: 'user', content: 'Hello!' }],
        })
      ).rejects.toThrow(XergonError);

      try {
        await client.chat.completions.create({
          model: 'llama-3.3-70b',
          messages: [{ role: 'user', content: 'Hello!' }],
        });
      } catch (err) {
        expect(err).toBeInstanceOf(XergonError);
        const xergonErr = err as XergonError;
        expect(xergonErr.type).toBe('unauthorized');
        expect(xergonErr.code).toBe(401);
        expect(xergonErr.isUnauthorized).toBe(true);
      }
    });

    it('throws XergonError with type rate_limit_error on 429', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 429,
        statusText: 'Too Many Requests',
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve({
          error: {
            type: 'rate_limit_error',
            message: 'Rate limit exceeded. Try again later.',
            code: 429,
          },
        }),
      }));

      // Disable retries so the error is thrown immediately
      const client = new XergonClient({ retries: false });

      try {
        await client.chat.completions.create({
          model: 'llama-3.3-70b',
          messages: [{ role: 'user', content: 'Hello!' }],
        });
        expect.fail('Should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(XergonError);
        const xergonErr = err as XergonError;
        expect(xergonErr.type).toBe('rate_limit_error');
        expect(xergonErr.code).toBe(429);
        expect(xergonErr.isRateLimited).toBe(true);
      }
    });

    it('throws XergonError with type internal_error on 500', async () => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        headers: new Headers({ 'content-type': 'application/json' }),
        json: () => Promise.resolve({
          error: {
            type: 'internal_error',
            message: 'Something went wrong on the relay',
            code: 500,
          },
        }),
      }));

      // Disable retries so the error is thrown immediately
      const client = new XergonClient({ retries: false });

      try {
        await client.chat.completions.create({
          model: 'llama-3.3-70b',
          messages: [{ role: 'user', content: 'Hello!' }],
        });
        expect.fail('Should have thrown');
      } catch (err) {
        expect(err).toBeInstanceOf(XergonError);
        const xergonErr = err as XergonError;
        expect(xergonErr.type).toBe('internal_error');
        expect(xergonErr.code).toBe(500);
      }
    });
  });

  describe('streaming completion', () => {
    it('returns an async iterable of chunks', async () => {
      const chunks = [
        { id: 'chatcmpl-abc', object: 'chat.completion.chunk', created: 1700000000, model: 'llama-3.3-70b', choices: [{ index: 0, delta: { role: 'assistant' }, finishReason: null }] },
        { id: 'chatcmpl-abc', object: 'chat.completion.chunk', created: 1700000000, model: 'llama-3.3-70b', choices: [{ index: 0, delta: { content: 'Hello' }, finishReason: null }] },
        { id: 'chatcmpl-abc', object: 'chat.completion.chunk', created: 1700000000, model: 'llama-3.3-70b', choices: [{ index: 0, delta: { content: '!' }, finishReason: null }] },
        { id: 'chatcmpl-abc', object: 'chat.completion.chunk', created: 1700000000, model: 'llama-3.3-70b', choices: [{ index: 0, delta: {}, finishReason: 'stop' }] },
      ];

      const sseData = chunks.map(c => `data: ${JSON.stringify(c)}\n\n`).join('') + 'data: [DONE]\n\n';

      const encoder = new TextEncoder();
      const readableStream = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode(sseData));
          controller.close();
        },
      });

      // Create a fresh stream for each call to avoid "locked" errors
      let streamCallCount = 0;
      vi.stubGlobal('fetch', vi.fn().mockImplementation(() => {
        streamCallCount++;
        return Promise.resolve({
          ok: true,
          status: 200,
          body: new ReadableStream({
            start(controller) {
              controller.enqueue(encoder.encode(sseData));
              controller.close();
            },
          }),
          headers: new Headers({ 'content-type': 'text/event-stream' }),
        });
      }));

      // Disable SSE retry to use simple streaming path
      const client = new XergonClient({ retries: false });
      const stream = await client.chat.completions.stream({
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello!' }],
      }, { sseRetry: false });

      const collected: any[] = [];
      for await (const chunk of stream) {
        collected.push(chunk);
      }

      expect(collected).toHaveLength(4);
      expect(collected[0].choices[0].delta.role).toBe('assistant');
      expect(collected[1].choices[0].delta.content).toBe('Hello');
      expect(collected[2].choices[0].delta.content).toBe('!');
      expect(collected[3].choices[0].finishReason).toBe('stop');
    });

    it('sends stream: true in the request body for streaming', async () => {
      const encoder = new TextEncoder();
      vi.stubGlobal('fetch', vi.fn().mockImplementation(() => {
        return Promise.resolve({
          ok: true,
          status: 200,
          body: new ReadableStream({
            start(controller) {
              controller.enqueue(encoder.encode('data: [DONE]\n\n'));
              controller.close();
            },
          }),
          headers: new Headers({ 'content-type': 'text/event-stream' }),
        });
      }));

      // Disable retries and SSE retry
      const client = new XergonClient({ retries: false });
      const iterable = await client.chat.completions.stream({
        model: 'llama-3.3-70b',
        messages: [{ role: 'user', content: 'Hello!' }],
      }, { sseRetry: false });

      // Consume the iterable to trigger the fetch
      for await (const _ of iterable) { /* consume */ }

      expect(fetch).toHaveBeenCalledTimes(1);
      const options = (fetch as ReturnType<typeof vi.fn>).mock.calls[0][1] as RequestInit;
      const body = JSON.parse(options.body as string);
      expect(body.stream).toBe(true);

      const headers = options.headers as Record<string, string>;
      expect(headers['Accept']).toBe('text/event-stream');
    });
  });
});
