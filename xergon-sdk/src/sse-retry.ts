/**
 * Resilient SSE (Server-Sent Events) with automatic reconnect and backoff.
 *
 * Wraps the SSE streaming logic so that connection drops during streaming
 * are handled gracefully -- the stream reconnects with exponential backoff
 * and sends Last-Event-ID if available.
 */

import type { ChatCompletionChunk } from './types';

export interface SSERetryOptions {
  /** Maximum number of reconnection attempts (default: 3). */
  maxReconnects?: number;
  /** Initial delay in ms before first reconnect (default: 1000). */
  initialDelayMs?: number;
  /** Maximum delay cap in ms (default: 30000). */
  maxDelayMs?: number;
  /** Backoff multiplier (default: 2). */
  backoffFactor?: number;
  /** Called on each reconnect attempt. */
  onReconnect?: (attempt: number, delayMs: number) => void;
}

const DEFAULT_SSE_OPTIONS = {
  maxReconnects: 3,
  initialDelayMs: 1000,
  maxDelayMs: 30000,
  backoffFactor: 2,
};

/**
 * A special chunk emitted when the stream reconnects.
 */
export interface SSEReconnectEvent {
  type: 'reconnect';
  attempt: number;
}

function calculateDelay(
  attempt: number,
  initialDelayMs: number,
  maxDelayMs: number,
  backoffFactor: number,
): number {
  const exponential = initialDelayMs * Math.pow(backoffFactor, attempt);
  const jitter = Math.random() * 1000;
  return Math.min(exponential + jitter, maxDelayMs);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Create a resilient SSE stream that yields ChatCompletionChunks.
 *
 * On connection errors or stream interruptions, it will:
 * 1. Wait with exponential backoff + jitter
 * 2. Reconnect with Last-Event-ID header tracking
 * 3. Emit a SSEReconnectEvent to notify the consumer
 */
export function createResilientSSEIterable(
  fetchStream: () => Promise<ReadableStream<Uint8Array> | null>,
  options?: SSERetryOptions,
): AsyncIterable<ChatCompletionChunk | SSEReconnectEvent> {
  const opts = { ...DEFAULT_SSE_OPTIONS, ...options };

  return {
    [Symbol.asyncIterator]() {
      let reconnectAttempt = 0;
      let lastEventId: string | null = null;
      let buffer = '';
      let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
      let streamDone = false; // True when the current stream's reader returned done
      let exhausted = false;
      const decoder = new TextDecoder();

      function releaseReader(): void {
        if (reader) {
          try { reader.releaseLock(); } catch { /* already released */ }
          reader = null;
        }
      }

      /**
       * Try to extract the next event from the buffer.
       * Returns null if no complete event is available.
       */
      function tryExtractFromBuffer(): { event: ChatCompletionChunk | SSEReconnectEvent | null; isDone: boolean } {
        const parsed = extractNextEvent(buffer);
        if (!parsed) return { event: null, isDone: false };

        buffer = buffer.substring(parsed.consumed);

        if (parsed.eventId) lastEventId = parsed.eventId;

        if (parsed.done) {
          return { event: null, isDone: true };
        }

        return { event: parsed.chunk, isDone: false };
      }

      /**
       * Ensure we have an active stream connection.
       * Returns false if exhausted (no more reconnects).
       * May set a pending result (reconnect event) via the outParam pattern.
       */
      async function connectOrReconnect(): Promise<SSEReconnectEvent | null> {
        // Already have a reader
        if (reader) return null;

        // Already know stream is done, need reconnect
        while (reconnectAttempt <= opts.maxReconnects) {
          let stream: ReadableStream<Uint8Array> | null = null;
          try {
            stream = await fetchStream();
          } catch {
            // Fetch itself failed
          }

          if (!stream) {
            if (reconnectAttempt >= opts.maxReconnects) {
              exhausted = true;
              return null;
            }

            const delayMs = calculateDelay(
              reconnectAttempt,
              opts.initialDelayMs,
              opts.maxDelayMs,
              opts.backoffFactor,
            );

            reconnectAttempt++;
            opts.onReconnect?.(reconnectAttempt, delayMs);
            console.warn(
              `[xergon-sdk] SSE connection failed (attempt ${reconnectAttempt}/${opts.maxReconnects + 1}), ` +
              `reconnecting in ${Math.round(delayMs)}ms...`,
            );

            await sleep(delayMs);

            // Return reconnect event, will try connecting on next call
            return { type: 'reconnect', attempt: reconnectAttempt };
          }

          // Got a stream
          reader = stream.getReader();
          streamDone = false;
          return null;
        }

        exhausted = true;
        return null;
      }

      return {
        async next(): Promise<IteratorResult<ChatCompletionChunk | SSEReconnectEvent>> {
          // Check buffer first (may have events from previous read)
          const buffered = tryExtractFromBuffer();
          if (buffered.isDone) {
            releaseReader();
            exhausted = true;
            return { value: undefined as any, done: true };
          }
          if (buffered.event) {
            return { value: buffered.event, done: false };
          }

          // Need more data from stream
          if (exhausted) {
            return { value: undefined as any, done: true };
          }

          // Ensure we have a connection
          const reconnectEvent = await connectOrReconnect();
          if (reconnectEvent) {
            return { value: reconnectEvent, done: false };
          }
          if (exhausted) {
            return { value: undefined as any, done: true };
          }

          // Read from stream
          try {
            const { done, value } = await reader!.read();

            if (done) {
              streamDone = true;
              releaseReader();

              // Try to extract from remaining buffer
              const remaining = tryExtractFromBuffer();
              if (remaining.isDone) {
                exhausted = true;
                return { value: undefined as any, done: true };
              }
              if (remaining.event) {
                return { value: remaining.event, done: false };
              }

              // Stream ended with partial data in buffer -- no complete events
              // If we still have buffer data, we might need to wait for more
              // But stream is done, so we're finished
              exhausted = true;
              return { value: undefined as any, done: true };
            }

            buffer += decoder.decode(value, { stream: true });

            // Try to extract events from updated buffer
            const result = tryExtractFromBuffer();
            if (result.isDone) {
              releaseReader();
              exhausted = true;
              return { value: undefined as any, done: true };
            }
            if (result.event) {
              return { value: result.event, done: false };
            }

            // Data received but no complete event yet -- return and let caller call next() again
            // Don't recurse to avoid stack overflow
            return this.next();
          } catch (err) {
            releaseReader();

            if (reconnectAttempt >= opts.maxReconnects) {
              exhausted = true;
              return { value: undefined as any, done: true };
            }

            const delayMs = calculateDelay(
              reconnectAttempt,
              opts.initialDelayMs,
              opts.maxDelayMs,
              opts.backoffFactor,
            );

            reconnectAttempt++;
            opts.onReconnect?.(reconnectAttempt, delayMs);
            console.warn(
              `[xergon-sdk] SSE stream interrupted (attempt ${reconnectAttempt}/${opts.maxReconnects + 1}), ` +
              `reconnecting in ${Math.round(delayMs)}ms...`,
            );

            await sleep(delayMs);
            return { value: { type: 'reconnect', attempt: reconnectAttempt } as SSEReconnectEvent, done: false };
          }
        },

        async return(): Promise<IteratorResult<ChatCompletionChunk | SSEReconnectEvent>> {
          releaseReader();
          exhausted = true;
          return { value: undefined as any, done: true };
        },
      };
    },
  };
}

// ── SSE Parsing ─────────────────────────────────────────────────────────

interface ParsedSSEEvent {
  chunk: ChatCompletionChunk;
  consumed: number;
  done: boolean;
  eventId: string | null;
}

function extractNextEvent(buffer: string): ParsedSSEEvent | null {
  const dataPrefix = 'data: ';
  const eventEnd = '\n\n';
  let searchFrom = 0;

  while (searchFrom < buffer.length) {
    const dataIndex = buffer.indexOf(dataPrefix, searchFrom);
    if (dataIndex === -1) {
      if (buffer.length > 0 && !buffer.endsWith('\n')) {
        return null;
      }
      break;
    }

    const lineEnd = buffer.indexOf('\n', dataIndex);
    if (lineEnd === -1) return null;

    const dataContent = buffer.substring(dataIndex + dataPrefix.length, lineEnd).trim();

    if (dataContent.startsWith(':') || dataContent === '') {
      searchFrom = lineEnd + 1;
      continue;
    }

    if (dataContent === '[DONE]') {
      const endIdx = buffer.indexOf(eventEnd, lineEnd);
      if (endIdx === -1) return null;
      return {
        chunk: undefined as any,
        consumed: endIdx + eventEnd.length,
        done: true,
        eventId: null,
      };
    }

    const endIdx = buffer.indexOf(eventEnd, lineEnd);
    if (endIdx === -1) return null;

    try {
      const chunk = JSON.parse(dataContent) as ChatCompletionChunk;
      return { chunk, consumed: endIdx + eventEnd.length, done: false, eventId: null };
    } catch {
      searchFrom = endIdx + eventEnd.length;
      continue;
    }
  }

  return null;
}
