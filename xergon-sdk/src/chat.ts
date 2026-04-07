/**
 * Chat completion methods -- streaming and non-streaming.
 */

import type {
  ChatCompletionParams,
  ChatCompletionResponse,
  ChatCompletionChunk,
} from './types';
import type { SSERetryOptions } from './sse-retry';
import { XergonClientCore } from './client';
import { XergonError } from './errors';
import { createResilientSSEIterable } from './sse-retry';

/**
 * Create a chat completion (non-streaming).
 */
export async function createChatCompletion(
  client: XergonClientCore,
  params: ChatCompletionParams,
  options?: { signal?: AbortSignal },
): Promise<ChatCompletionResponse> {
  return client.post<ChatCompletionResponse>(
    '/v1/chat/completions',
    { ...params, stream: false },
    { signal: options?.signal },
  );
}

/**
 * Stream a chat completion via SSE (Server-Sent Events).
 *
 * Returns an AsyncIterable that yields ChatCompletionChunk objects.
 * Each chunk contains one or more choices with delta updates.
 *
 * Supports automatic reconnection on stream interruption via `sseRetry` option.
 */
export async function streamChatCompletion(
  client: XergonClientCore,
  params: ChatCompletionParams,
  options?: { signal?: AbortSignal; sseRetry?: SSERetryOptions | false },
): Promise<AsyncIterable<ChatCompletionChunk>> {
  const url = `${client.getBaseUrl()}/v1/chat/completions`;
  const bodyStr = JSON.stringify({ ...params, stream: true });

  const fetchStream = async (): Promise<ReadableStream<Uint8Array> | null> => {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'text/event-stream',
    };

    const authHeaders = await client['buildAuthHeaders'](
      'POST',
      '/v1/chat/completions',
      bodyStr,
    );
    Object.assign(headers, authHeaders);

    const res = await fetch(url, {
      method: 'POST',
      headers,
      body: bodyStr,
      signal: options?.signal,
    });

    if (!res.ok) {
      let errorData: unknown;
      try {
        errorData = await res.json();
      } catch {
        errorData = { message: res.statusText };
      }
      throw XergonError.fromResponse(errorData);
    }

    if (!res.body) {
      throw new XergonError({
        type: 'service_unavailable',
        message: 'Response body is not readable -- streaming not supported',
        code: 503,
      });
    }

    return res.body;
  };

  // If SSE retry is disabled, use the simple non-resilient path
  if (options?.sseRetry === false) {
    const stream = await fetchStream();
    if (!stream) {
      throw new XergonError({
        type: 'service_unavailable',
        message: 'Failed to establish SSE connection',
        code: 503,
      });
    }
    return createSSEIterable(stream);
  }

  // Use resilient SSE with reconnect support
  const resilientIterable = createResilientSSEIterable(fetchStream, options?.sseRetry);

  // Filter out SSEReconnectEvent, yielding only ChatCompletionChunk
  return createFilteredIterable(resilientIterable);
}

/**
 * Filter an iterable to only yield ChatCompletionChunk, skipping SSEReconnectEvent.
 */
function createFilteredIterable(
  source: AsyncIterable<ChatCompletionChunk | { type: 'reconnect'; attempt: number }>,
): AsyncIterable<ChatCompletionChunk> {
  return {
    [Symbol.asyncIterator]() {
      const inner = source[Symbol.asyncIterator]();
      return {
        async next(): Promise<IteratorResult<ChatCompletionChunk>> {
          while (true) {
            const result = await inner.next();
            if (result.done) {
              return { value: undefined as any, done: true };
            }
            // Skip reconnect events, only yield chunks
            if ('type' in result.value && result.value.type === 'reconnect') {
              continue;
            }
            return { value: result.value as ChatCompletionChunk, done: false };
          }
        },
        async return(): Promise<IteratorResult<ChatCompletionChunk>> {
          return (await inner.return?.()) as IteratorResult<ChatCompletionChunk> ?? { value: undefined as any, done: true };
        },
      };
    },
  };
}

/**
 * Parse a ReadableStream of SSE data into an AsyncIterable of ChatCompletionChunk.
 */
function createSSEIterable(
  stream: ReadableStream<Uint8Array>,
): AsyncIterable<ChatCompletionChunk> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();

  return {
    [Symbol.asyncIterator]() {
      let buffer = '';

      return {
        async next(): Promise<IteratorResult<ChatCompletionChunk>> {
          while (true) {
            // Check buffer for complete events first
            const event = extractNextEvent(buffer);
            if (event !== null) {
              buffer = buffer.substring(event.consumed);
              if (event.done) {
                return { value: undefined as any, done: true };
              }
              return { value: event.chunk, done: false };
            }

            // Read more data
            const { done, value } = await reader.read();
            if (done) {
              // Process remaining buffer
              const remaining = extractNextEvent(buffer);
              if (remaining && !remaining.done) {
                return { value: remaining.chunk, done: false };
              }
              return { value: undefined as any, done: true };
            }
            buffer += decoder.decode(value, { stream: true });
          }
        },

        async return(): Promise<IteratorResult<ChatCompletionChunk>> {
          reader.releaseLock();
          return { value: undefined as any, done: true };
        },
      };
    },
  };
}

interface SSEEvent {
  chunk: ChatCompletionChunk;
  consumed: number;
  done: boolean;
}

/**
 * Extract the next complete SSE event from the buffer.
 */
function extractNextEvent(
  buffer: string,
): SSEEvent | null {
  // SSE format: "data: {...}\n\n"
  const dataPrefix = 'data: ';
  const eventEnd = '\n\n';

  let searchFrom = 0;

  while (searchFrom < buffer.length) {
    const dataIndex = buffer.indexOf(dataPrefix, searchFrom);
    if (dataIndex === -1) {
      // No more data lines in buffer
      if (buffer.length > 0 && !buffer.endsWith('\n')) {
        // Incomplete -- wait for more data
        return null;
      }
      break;
    }

    const lineEnd = buffer.indexOf('\n', dataIndex);
    if (lineEnd === -1) {
      // Incomplete line -- wait for more data
      return null;
    }

    const dataContent = buffer.substring(dataIndex + dataPrefix.length, lineEnd).trim();

    // Skip comments and empty lines
    if (dataContent.startsWith(':') || dataContent === '') {
      searchFrom = lineEnd + 1;
      continue;
    }

    // Check for [DONE] sentinel
    if (dataContent === '[DONE]') {
      // Find the end of this event
      const endIdx = buffer.indexOf(eventEnd, lineEnd);
      if (endIdx === -1) {
        return null; // Incomplete
      }
      return {
        chunk: undefined as any,
        consumed: endIdx + eventEnd.length,
        done: true,
      };
    }

    // Find event end
    const endIdx = buffer.indexOf(eventEnd, lineEnd);
    if (endIdx === -1) {
      // Event not complete yet
      return null;
    }

    try {
      const chunk = JSON.parse(dataContent) as ChatCompletionChunk;
      return {
        chunk,
        consumed: endIdx + eventEnd.length,
        done: false,
      };
    } catch {
      // Malformed JSON -- skip this event
      searchFrom = endIdx + eventEnd.length;
      continue;
    }
  }

  return null;
}
