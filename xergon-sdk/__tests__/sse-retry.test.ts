/**
 * Tests for createResilientSSEIterable -- SSE reconnect with backoff.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createResilientSSEIterable, SSEReconnectEvent } from '../src/sse-retry';

function createSSEStream(chunks: object[]): ReadableStream<Uint8Array> {
  const sseData = chunks
    .map(c => `data: ${JSON.stringify(c)}\n\n`)
    .join('') + 'data: [DONE]\n\n';

  return new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(sseData));
      controller.close();
    },
  });
}

/**
 * Helper: collect all values from an async iterable using fake timers.
 * Advances timers by small increments to let microtasks resolve.
 */
async function collectAll(
  iterable: AsyncIterable<any>,
  maxTimerAdvance: number = 60000,
): Promise<{ values: any[]; done: boolean }> {
  const values: any[] = [];
  const iterator = iterable[Symbol.asyncIterator]();

  let totalAdvanced = 0;
  const step = 100;

  while (totalAdvanced < maxTimerAdvance) {
    // Try to get next value
    let settled = false;
    let result: IteratorResult<any>;

    const p = iterator.next().then(r => {
      result = r;
      settled = true;
      return r;
    });

    // Advance timers in small steps until the promise settles
    while (!settled && totalAdvanced < maxTimerAdvance) {
      await vi.advanceTimersByTimeAsync(step);
      totalAdvanced += step;
      // Give microtasks a chance to run
      await Promise.resolve();
    }

    if (!settled) {
      break;
    }

    if (result!.done) {
      return { values, done: true };
    }

    values.push(result!.value);
  }

  return { values, done: false };
}

describe('createResilientSSEIterable', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('successful stream', () => {
    it('yields all chunks from a successful stream', async () => {
      const chunks = [
        { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: 'Hello' }, finishReason: null }] },
        { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: ' world' }, finishReason: null }] },
        { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: {}, finishReason: 'stop' }] },
      ];

      const fetchStream = vi.fn().mockResolvedValue(createSSEStream(chunks));

      const iterable = createResilientSSEIterable(fetchStream);
      const { values, done } = await collectAll(iterable);

      expect(fetchStream).toHaveBeenCalledTimes(1);
      expect(done).toBe(true);
      expect(values).toHaveLength(3);
      expect(values[0].choices[0].delta.content).toBe('Hello');
      expect(values[1].choices[0].delta.content).toBe(' world');
      expect(values[2].choices[0].finishReason).toBe('stop');
    });
  });

  describe('reconnect on connection failure', () => {
    it('reconnects when fetchStream returns null and then succeeds', async () => {
      const chunks = [
        { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: 'recovered' }, finishReason: null }] },
      ];

      let callCount = 0;
      const fetchStream = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve(null);
        }
        return Promise.resolve(createSSEStream(chunks));
      });

      const onReconnect = vi.fn();
      const iterable = createResilientSSEIterable(fetchStream, { onReconnect });
      const { values, done } = await collectAll(iterable);

      expect(done).toBe(true);
      expect(fetchStream).toHaveBeenCalledTimes(2); // initial fail + reconnect success
      expect(onReconnect).toHaveBeenCalledTimes(1);

      // Should have a reconnect event followed by the chunk
      const reconnectEvents = values.filter(v => v && v.type === 'reconnect');
      const dataEvents = values.filter(v => v && v.choices);
      expect(reconnectEvents).toHaveLength(1);
      expect(reconnectEvents[0].attempt).toBe(1);
      expect(dataEvents).toHaveLength(1);
      expect(dataEvents[0].choices[0].delta.content).toBe('recovered');
    });

    it('reconnects when fetchStream throws and then succeeds', async () => {
      const chunks = [
        { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: 'ok' }, finishReason: null }] },
      ];

      let callCount = 0;
      const fetchStream = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new TypeError('fetch failed'));
        }
        return Promise.resolve(createSSEStream(chunks));
      });

      const iterable = createResilientSSEIterable(fetchStream);
      const { values, done } = await collectAll(iterable);

      expect(done).toBe(true);
      expect(fetchStream).toHaveBeenCalledTimes(2);

      const reconnectEvents = values.filter(v => v && v.type === 'reconnect');
      expect(reconnectEvents).toHaveLength(1);
    });
  });

  describe('max reconnects exhausted', () => {
    it('ends the stream when max reconnects is reached', async () => {
      const fetchStream = vi.fn().mockResolvedValue(null);
      const onReconnect = vi.fn();

      const iterable = createResilientSSEIterable(fetchStream, {
        maxReconnects: 2,
        onReconnect,
      });

      const { values, done } = await collectAll(iterable);

      expect(done).toBe(true);
      // initial(1) + reconnect1(2) + reconnect2(3) = 3 calls
      expect(fetchStream).toHaveBeenCalledTimes(3);
      expect(onReconnect).toHaveBeenCalledTimes(2);

      // All values should be reconnect events
      const reconnectEvents = values.filter(v => v && v.type === 'reconnect');
      expect(reconnectEvents).toHaveLength(2);
      expect(reconnectEvents[0].attempt).toBe(1);
      expect(reconnectEvents[1].attempt).toBe(2);
    });

    it('respects maxReconnects: 0 (no reconnects)', async () => {
      const fetchStream = vi.fn().mockResolvedValue(null);

      const iterable = createResilientSSEIterable(fetchStream, { maxReconnects: 0 });
      const { values, done } = await collectAll(iterable);

      expect(done).toBe(true);
      expect(fetchStream).toHaveBeenCalledTimes(1);
      expect(values).toHaveLength(0);
    });
  });

  describe('backoff timing', () => {
    it('onReconnect is called with increasing delays', async () => {
      const fetchStream = vi.fn().mockResolvedValue(null);
      const delays: number[] = [];
      const onReconnect = vi.fn((_, delayMs) => delays.push(delayMs));

      const iterable = createResilientSSEIterable(fetchStream, {
        maxReconnects: 3,
        onReconnect,
        initialDelayMs: 1000,
        backoffFactor: 2,
      });

      const { done } = await collectAll(iterable);
      expect(done).toBe(true);
      expect(onReconnect).toHaveBeenCalledTimes(3);

      // Delays should generally increase (allowing for jitter)
      // First delay ~1000, second ~2000, third ~4000
      expect(delays[0]).toBeGreaterThanOrEqual(1000);
      expect(delays[0]).toBeLessThan(2000); // 1000 + jitter(0-1000)
      expect(delays[1]).toBeGreaterThanOrEqual(2000);
      expect(delays[2]).toBeGreaterThanOrEqual(4000);
    });
  });

  describe('stream interruption (read error)', () => {
    it('emits reconnect event when stream read fails mid-stream', async () => {
      const chunk1 = { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: 'before' }, finishReason: null }] };
      const chunk2 = { id: 'abc', object: 'chat.completion.chunk', created: 1, model: 'm', choices: [{ index: 0, delta: { content: 'after reconnect' }, finishReason: null }] };

      let callCount = 0;
      const fetchStream = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          // First stream: send one chunk then error
          const encoder = new TextEncoder();
          return new ReadableStream({
            start(controller) {
              controller.enqueue(encoder.encode(`data: ${JSON.stringify(chunk1)}\n\n`));
              // Simulate error after sending data
              setTimeout(() => {
                controller.error(new TypeError('Connection reset'));
              }, 0);
            },
          });
        }
        // Second stream: successful
        return Promise.resolve(createSSEStream([chunk2]));
      });

      const onReconnect = vi.fn();
      const iterable = createResilientSSEIterable(fetchStream, { onReconnect });
      const { values, done } = await collectAll(iterable);

      expect(done).toBe(true);
      expect(fetchStream).toHaveBeenCalledTimes(2);

      const dataEvents = values.filter(v => v && v.choices);
      const reconnectEvents = values.filter(v => v && v.type === 'reconnect');

      expect(dataEvents).toHaveLength(2);
      expect(dataEvents[0].choices[0].delta.content).toBe('before');
      expect(dataEvents[1].choices[0].delta.content).toBe('after reconnect');
      expect(reconnectEvents).toHaveLength(1);
      expect(onReconnect).toHaveBeenCalledTimes(1);
    });
  });
});
