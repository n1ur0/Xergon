/**
 * Tests for XergonWebSocketClient -- connection, messaging, reconnect, keepalive, queue.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { XergonWebSocketClient } from '../src/ws';
import type { WSOptions, ChatChunk } from '../src/ws';

// ── Mock WebSocket ───────────────────────────────────────────────────

let wsInstances: Array<{
  url: string;
  onopen: (() => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
  onerror: (() => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  readyState: number;
  send: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
}> = [];

/**
 * Install a mock WebSocket that does NOT auto-fire onopen.
 * Tests must call triggerOpen() manually.
 */
function installMockWebSocket() {
  wsInstances = [];

  class MockWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;

    url: string;
    onopen: (() => void) | null = null;
    onmessage: ((ev: MessageEvent) => void) | null = null;
    onerror: (() => void) | null = null;
    onclose: ((ev: CloseEvent) => void) | null = null;
    readyState = MockWebSocket.CONNECTING;
    send: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;

    constructor(url: string) {
      this.url = url;
      this.send = vi.fn();
      this.close = vi.fn();
      wsInstances.push(this);
    }

    triggerOpen() {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.();
    }

    triggerMessage(data: string) {
      this.onmessage?.(new MessageEvent('message', { data }));
    }

    triggerClose(code: number, reason = '') {
      this.readyState = MockWebSocket.CLOSED;
      // CloseEvent may not be available in Node; create a simple object
      const evt = { type: 'close', code, reason, wasClean: code === 1000 } as CloseEvent;
      this.onclose?.(evt);
    }

    triggerError() {
      this.onerror?.();
    }
  }

  return MockWebSocket;
}

// ── Tests ────────────────────────────────────────────────────────────

describe('XergonWebSocketClient', () => {
  let originalWS: typeof globalThis.WebSocket;
  let MockWS: ReturnType<typeof installMockWebSocket>;

  beforeEach(() => {
    vi.useFakeTimers();
    MockWS = installMockWebSocket();
    originalWS = globalThis.WebSocket;
    (globalThis as Record<string, unknown>).WebSocket = MockWS;
  });

  afterEach(() => {
    vi.useRealTimers();
    (globalThis as Record<string, unknown>).WebSocket = originalWS;
    wsInstances = [];
  });

/** Get the most recently created WS mock */
function lastWS() {
  return wsInstances[wsInstances.length - 1] as any;
}

  /** Connect client and manually fire open event */
  async function connectClient(
    url = 'ws://localhost:8080/v1/chat/ws',
    opts?: WSOptions,
  ): Promise<XergonWebSocketClient> {
    const client = new XergonWebSocketClient(url, opts);
    const p = client.connect();
    // The mock WS is created synchronously, trigger its open
    lastWS().triggerOpen();
    await p;
    return client;
  }

  // ── Creation & Connection ─────────────────────────────────────────

  it('creates with correct URL and default options', () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws');
    expect(client.getState()).toBe('disconnected');
  });

  it('connects and transitions to connected state', async () => {
    const client = await connectClient();
    expect(client.getState()).toBe('connected');
  });

  it('appends auth token to URL when provided', async () => {
    await connectClient('ws://localhost:8080/v1/chat/ws', {
      authToken: 'my-jwt',
    });
    expect(lastWS().url).toContain('token=my-jwt');
  });

  it('resolves immediately if already connected', async () => {
    const client = await connectClient();
    await expect(client.connect()).resolves.toBeUndefined();
    expect(client.getState()).toBe('connected');
  });

  // ── State Transitions ─────────────────────────────────────────────

  it('transitions through connecting -> connected states', async () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws');
    const p = client.connect();
    expect(client.getState()).toBe('connecting');
    lastWS().triggerOpen();
    await p;
    expect(client.getState()).toBe('connected');
  });

  // ── Message Sending ───────────────────────────────────────────────

  it('sends chat completion messages', async () => {
    const client = await connectClient();
    client.sendChat({
      model: 'llama-3.3-70b',
      messages: [{ role: 'user', content: 'Hello' }],
    });

    expect(lastWS().send).toHaveBeenCalledTimes(1);
    const sent = JSON.parse(lastWS().send.mock.calls[0][0] as string);
    expect(sent.type).toBe('chat.completion');
    expect(sent.model).toBe('llama-3.3-70b');
  });

  it('sends raw string messages', async () => {
    const client = await connectClient();
    client.send('raw message');
    expect(lastWS().send).toHaveBeenCalledWith('raw message');
  });

  it('sends raw object messages (JSON serialized)', async () => {
    const client = await connectClient();
    client.send({ type: 'ping', data: 42 });
    expect(lastWS().send).toHaveBeenCalledWith('{"type":"ping","data":42}');
  });

  // ── Message Receiving ─────────────────────────────────────────────

  it('emits message events for JSON chunks', async () => {
    const client = await connectClient();
    const chunks: ChatChunk[] = [];
    client.onMessage((chunk) => chunks.push(chunk));

    const chunk: ChatChunk = {
      id: 'chatcmpl-123',
      model: 'llama-3.3-70b',
      choices: [{
        index: 0,
        delta: { content: 'Hello' },
        finish_reason: null,
      }],
    };

    lastWS().triggerMessage(JSON.stringify(chunk));

    expect(chunks).toHaveLength(1);
    expect(chunks[0].id).toBe('chatcmpl-123');
    expect(chunks[0].choices[0].delta.content).toBe('Hello');
  });

  it('emits error events for error messages from server', async () => {
    const client = await connectClient();
    const errors: import('../src/ws').WSError[] = [];
    client.onError((err) => errors.push(err));

    lastWS().triggerMessage(JSON.stringify({
      error: { code: 4001, message: 'Invalid request', reconnectable: false },
    }));

    expect(errors).toHaveLength(1);
    expect(errors[0].code).toBe(4001);
    expect(errors[0].message).toBe('Invalid request');
    expect(errors[0].reconnectable).toBe(false);
  });

  it('ignores non-JSON messages', async () => {
    const client = await connectClient();
    const chunks: ChatChunk[] = [];
    client.onMessage((chunk) => chunks.push(chunk));

    lastWS().triggerMessage('not json');
    expect(chunks).toHaveLength(0);
  });

  // ── Connection Events ─────────────────────────────────────────────

  it('emits open event on connection', async () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws');
    const openEvents: void[] = [];
    client.onOpen(() => openEvents.push(undefined));
    const p = client.connect();
    lastWS().triggerOpen();
    await p;
    expect(openEvents).toHaveLength(1);
  });

  it('emits close event on disconnection', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
    });
    const closeEvents: CloseEvent[] = [];
    client.onClose((ev) => closeEvents.push(ev));

    lastWS().triggerClose(1000, 'Normal');
    expect(closeEvents).toHaveLength(1);
    expect(closeEvents[0].code).toBe(1000);
  });

  // ── Disconnect ────────────────────────────────────────────────────

  it('disconnects cleanly', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
    });
    client.disconnect();
    expect(client.getState()).toBe('disconnected');
    expect(lastWS().close).toHaveBeenCalledWith(1000, 'Client disconnect');
  });

  it('disconnect is a no-op when already disconnected', () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws');
    client.disconnect();
    expect(client.getState()).toBe('disconnected');
  });

  // ── Reconnection ──────────────────────────────────────────────────

  it('attempts reconnect on unexpected close', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
      maxReconnectAttempts: 3,
      reconnectDelayMs: 1000,
    });

    const countBefore = wsInstances.length;
    lastWS().triggerClose(1006);
    expect(client.getState()).toBe('disconnected');

    // Advance to first reconnect (1000ms)
    await vi.advanceTimersByTimeAsync(1000);
    // New WS instance should be created
    expect(wsInstances.length).toBe(countBefore + 1);
    // Trigger open for the new connection
    wsInstances[wsInstances.length - 1]!.onopen?.();
    expect(client.getState()).toBe('connected');
  });

  it('respects maxReconnectAttempts for consecutive failures', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
      maxReconnectAttempts: 2,
      reconnectDelayMs: 100,
    });

    const countAfterInit = wsInstances.length;

    // Close and let 1st reconnect attempt fire (but don't trigger open -- simulates failure)
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(100);
    expect(wsInstances.length).toBe(countAfterInit + 1);

    // 1st reconnect also closes (failure), let 2nd attempt fire
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(200);
    expect(wsInstances.length).toBe(countAfterInit + 2);

    // 2nd reconnect also closes -- max reached (2 consecutive failures)
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(60000);
    // No more reconnect attempts
    expect(wsInstances.length).toBe(countAfterInit + 2);
  });

  it('does not reconnect on intentional disconnect', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
    });

    client.disconnect();
    expect(client.getState()).toBe('disconnected');

    const countAfterDisconnect = wsInstances.length;
    await vi.advanceTimersByTimeAsync(60000);
    expect(wsInstances.length).toBe(countAfterDisconnect);
  });

  // ── Ping/Pong Keepalive ───────────────────────────────────────────

  it('sends ping frames at configured interval', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      pingIntervalMs: 5000,
      pongTimeoutMs: 2000,
    });

    expect(lastWS().send).not.toHaveBeenCalledWith('__ping__');

    await vi.advanceTimersByTimeAsync(5000);
    expect(lastWS().send).toHaveBeenCalledWith('__ping__');
  });

  it('handles pong responses and resets pong timer', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      pingIntervalMs: 5000,
      pongTimeoutMs: 3000,
    });

    // Trigger ping
    await vi.advanceTimersByTimeAsync(5000);
    expect(lastWS().send).toHaveBeenCalledWith('__ping__');

    // Send pong before timeout
    lastWS().triggerMessage('__pong__');

    // Advance past pong timeout -- connection should still be open
    await vi.advanceTimersByTimeAsync(3000);
    expect(client.getState()).toBe('connected');
  });

  it('closes connection on pong timeout', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
      pingIntervalMs: 5000,
      pongTimeoutMs: 3000,
    });

    // Trigger ping
    await vi.advanceTimersByTimeAsync(5000);
    lastWS().send.mockClear();

    // Don't send pong, advance past timeout
    await vi.advanceTimersByTimeAsync(3000);

    expect(lastWS().close).toHaveBeenCalledWith(4000, 'Pong timeout');
  });

  it('stops keepalive on disconnect', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
      pingIntervalMs: 5000,
    });

    client.disconnect();
    await vi.advanceTimersByTimeAsync(20000);

    // No pings should have been sent (we clear on disconnect)
    const pingCalls = lastWS().send.mock.calls.filter(
      (c: unknown[]) => c[0] === '__ping__',
    );
    expect(pingCalls).toHaveLength(0);
  });

  // ── Message Queue ─────────────────────────────────────────────────

  it('queues messages when not connected and drains on connect', async () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
    });

    // Send before connecting
    client.send({ type: 'queued', data: 'message1' });
    client.send({ type: 'queued', data: 'message2' });

    // Connect
    const p = client.connect();
    lastWS().triggerOpen();
    await p;

    expect(lastWS().send).toHaveBeenCalledTimes(2);
    expect(JSON.parse(lastWS().send.mock.calls[0][0])).toEqual({ type: 'queued', data: 'message1' });
    expect(JSON.parse(lastWS().send.mock.calls[1][0])).toEqual({ type: 'queued', data: 'message2' });
  });

  it('queues messages sent before connect and drains on connect', async () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
    });

    // Send before connecting -- should be queued
    client.send({ type: 'queued', value: 42 });

    // Now connect
    const p = client.connect();
    const ws = wsInstances[wsInstances.length - 1] as any;
    ws.triggerOpen();
    await p;

    expect(ws.send).toHaveBeenCalledTimes(1);
    const sent = JSON.parse(ws.send.mock.calls[0][0] as string);
    expect(sent.type).toBe('queued');
    expect(sent.value).toBe(42);
  });

  it('does not queue messages when reconnect is disabled', () => {
    const client = new XergonWebSocketClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
    });

    // Silently dropped
    client.send({ type: 'dropped' });
    expect(client.getState()).toBe('disconnected');
  });

  // ── removeAllListeners ────────────────────────────────────────────

  it('removes all listeners and stops receiving events', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: false,
    });

    const chunks: ChatChunk[] = [];
    client.onMessage((chunk) => chunks.push(chunk));

    const chunk: ChatChunk = {
      id: 'test',
      model: 'test-model',
      choices: [{ index: 0, delta: { content: 'hi' }, finish_reason: null }],
    };
    lastWS().triggerMessage(JSON.stringify(chunk));
    expect(chunks).toHaveLength(1);

    client.removeAllListeners();

    lastWS().triggerMessage(JSON.stringify(chunk));
    expect(chunks).toHaveLength(1); // Not incremented
  });

  // ── Exponential Backoff ───────────────────────────────────────────

  it('uses exponential backoff for reconnect delays', async () => {
    const client = await connectClient('ws://localhost:8080/v1/chat/ws', {
      reconnect: true,
      maxReconnectAttempts: 3,
      reconnectDelayMs: 1000,
    });

    const countAfterInit = wsInstances.length;

    // Close -> 1st reconnect at 1000ms (don't open -- failure)
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(999);
    expect(wsInstances.length).toBe(countAfterInit); // Not yet
    await vi.advanceTimersByTimeAsync(1);
    expect(wsInstances.length).toBe(countAfterInit + 1); // Reconnect fired

    // Close -> 2nd reconnect at 2000ms (exponential: 1000 * 2)
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(1999);
    expect(wsInstances.length).toBe(countAfterInit + 1);
    await vi.advanceTimersByTimeAsync(1);
    expect(wsInstances.length).toBe(countAfterInit + 2);

    // Close -> 3rd reconnect at 4000ms (exponential: 1000 * 4)
    lastWS().triggerClose(1006);
    await vi.advanceTimersByTimeAsync(3999);
    expect(wsInstances.length).toBe(countAfterInit + 2);
    await vi.advanceTimersByTimeAsync(1);
    expect(wsInstances.length).toBe(countAfterInit + 3);
  });
});
