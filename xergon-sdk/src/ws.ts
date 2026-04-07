/**
 * WebSocket client for Xergon relay -- provides real-time chat streaming
 * over the /v1/chat/ws endpoint.
 *
 * Features:
 * - Auto-reconnect with exponential backoff
 * - Ping/pong keepalive
 * - Message queue (buffers during reconnection)
 * - Connection state management
 * - Clean disconnect with pending message drain
 */

import type { ChatCompletionParams, ChatCompletionChunk } from './types';

// ── Types ─────────────────────────────────────────────────────────────

export interface WSOptions {
  /** Enable auto-reconnect on disconnection. Default: true */
  reconnect?: boolean;
  /** Maximum number of reconnect attempts. Default: 5 */
  maxReconnectAttempts?: number;
  /** Initial delay between reconnects in ms. Default: 1000 */
  reconnectDelayMs?: number;
  /** Send ping frames at this interval. Default: 30000 */
  pingIntervalMs?: number;
  /** Close connection if pong not received within this window. Default: 5000 */
  pongTimeoutMs?: number;
  /** JWT token for authenticated connections. */
  authToken?: string;
}

export type WSConnectionState =
  | 'connecting'
  | 'connected'
  | 'disconnecting'
  | 'disconnected';

export interface ChatChunk {
  id: string;
  model: string;
  choices: Array<{
    index: number;
    delta: { role?: string; content?: string };
    finish_reason: string | null;
  }>;
}

export interface WSError {
  code: number;
  message: string;
  reconnectable: boolean;
}

// ── Client ────────────────────────────────────────────────────────────

type MessageCallback = (chunk: ChatChunk) => void;
type ErrorCallback = (error: WSError) => void;
type OpenCallback = () => void;
type CloseCallback = (event: CloseEvent) => void;

const DEFAULTS: Required<WSOptions> = {
  reconnect: true,
  maxReconnectAttempts: 5,
  reconnectDelayMs: 1000,
  pingIntervalMs: 30000,
  pongTimeoutMs: 5000,
  authToken: '',
};

export class XergonWebSocketClient {
  private url: string;
  private options: Required<WSOptions>;

  private ws: WebSocket | null = null;
  private state: WSConnectionState = 'disconnected';
  private reconnectAttempts = 0;

  // Timers
  private pingTimer: ReturnType<typeof setInterval> | null = null;
  private pongTimer: ReturnType<typeof setTimeout> | null = null;

  // Listeners
  private messageListeners: MessageCallback[] = [];
  private errorListeners: ErrorCallback[] = [];
  private openListeners: OpenCallback[] = [];
  private closeListeners: CloseCallback[] = [];

  // Message queue for buffering during reconnection
  private messageQueue: unknown[] = [];
  private intentionalClose = false;

  constructor(url: string, options?: WSOptions) {
    this.url = url;
    this.options = { ...DEFAULTS, ...options };
  }

  // ── Connection ──────────────────────────────────────────────────────

  /**
   * Connect to the relay WebSocket endpoint.
   * Resolves when the WebSocket open event fires.
   */
  connect(): Promise<void> {
    if (this.state === 'connected' || this.state === 'connecting') {
      return Promise.resolve();
    }

    this.intentionalClose = false;
    this.setState('connecting');

    return new Promise<void>((resolve, reject) => {
      try {
        const wsUrl = this.buildUrl();
        this.ws = new WebSocket(wsUrl);

        const timeout = setTimeout(() => {
          this.ws?.close();
          reject(new Error('WebSocket connection timed out'));
        }, 10000);

        this.ws.onopen = () => {
          clearTimeout(timeout);
          this.setState('connected');
          this.reconnectAttempts = 0;
          this.startKeepalive();
          this.drainQueue();
          this.emitOpen();
          resolve();
        };

        this.ws.onmessage = (event: MessageEvent) => {
          this.handleMessage(event);
        };

        this.ws.onerror = () => {
          clearTimeout(timeout);
          // onerror fires before onclose; errors are emitted from onclose
        };

        this.ws.onclose = (event: CloseEvent) => {
          clearTimeout(timeout);
          this.stopKeepalive();
          this.handleClose(event);
        };
      } catch (err) {
        reject(err);
      }
    });
  }

  // ── Send ────────────────────────────────────────────────────────────

  /**
   * Send a chat completion request over the WebSocket.
   * If not connected, the message is queued and sent on reconnect.
   */
  sendChat(request: ChatCompletionParams): void {
    this.send({ type: 'chat.completion', ...request });
  }

  /**
   * Send a raw message over the WebSocket.
   * If not connected, the message is queued and sent on reconnect.
   */
  send(data: unknown): void {
    if (this.state === 'connected' && this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(typeof data === 'string' ? data : JSON.stringify(data));
    } else if (this.options.reconnect) {
      this.messageQueue.push(data);
    }
  }

  // ── State ───────────────────────────────────────────────────────────

  getState(): WSConnectionState {
    return this.state;
  }

  // ── Disconnect ──────────────────────────────────────────────────────

  /**
   * Gracefully disconnect. Drains pending messages before closing.
   */
  disconnect(): void {
    this.intentionalClose = true;
    this.stopKeepalive();

    if (this.state === 'disconnected') {
      return;
    }

    this.setState('disconnecting');

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      // Try to drain queue before closing
      if (this.messageQueue.length > 0) {
        this.drainQueue();
      }
      this.ws.close(1000, 'Client disconnect');
    }

    // If still connecting, force close
    if (this.ws && this.ws.readyState === WebSocket.CONNECTING) {
      this.ws.close(1000, 'Client disconnect');
    }

    this.messageQueue = [];
    this.ws = null;
    this.setState('disconnected');
  }

  // ── Event Listeners ─────────────────────────────────────────────────

  onMessage(callback: MessageCallback): void {
    this.messageListeners.push(callback);
  }

  onError(callback: ErrorCallback): void {
    this.errorListeners.push(callback);
  }

  onOpen(callback: OpenCallback): void {
    this.openListeners.push(callback);
  }

  onClose(callback: CloseCallback): void {
    this.closeListeners.push(callback);
  }

  /**
   * Remove all event listeners and clean up timers.
   */
  removeAllListeners(): void {
    this.messageListeners = [];
    this.errorListeners = [];
    this.openListeners = [];
    this.closeListeners = [];
    this.stopKeepalive();
  }

  // ── Internal ────────────────────────────────────────────────────────

  private buildUrl(): string {
    const url = new URL(this.url);
    if (this.options.authToken) {
      url.searchParams.set('token', this.options.authToken);
    }
    return url.toString();
  }

  private setState(s: WSConnectionState): void {
    this.state = s;
  }

  private handleMessage(event: MessageEvent): void {
    // Handle pong
    if (typeof event.data === 'string' && event.data === '__pong__') {
      this.resetPongTimer();
      return;
    }

    let parsed: unknown;
    try {
      parsed = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
    } catch {
      // Non-JSON text message -- ignore
      return;
    }

    // Check if this is an error message from the server
    if (
      parsed &&
      typeof parsed === 'object' &&
      'error' in (parsed as Record<string, unknown>)
    ) {
      const errObj = (parsed as { error: Record<string, unknown> }).error;
      const wsError: WSError = {
        code: typeof errObj.code === 'number' ? errObj.code : 1011,
        message: typeof errObj.message === 'string' ? errObj.message : 'Unknown WebSocket error',
        reconnectable: typeof errObj.reconnectable === 'boolean' ? errObj.reconnectable : true,
      };
      this.emitError(wsError);
      return;
    }

    // Normal chat chunk
    const chunk = parsed as ChatChunk;
    this.emitMessage(chunk);
  }

  private handleClose(event: CloseEvent): void {
    const wasActive = this.state === 'connected' || this.state === 'connecting';
    this.ws = null;
    this.setState('disconnected');
    this.emitClose(event);

    // Attempt reconnect
    if (
      !this.intentionalClose &&
      this.options.reconnect &&
      wasActive &&
      this.reconnectAttempts < this.options.maxReconnectAttempts
    ) {
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    this.reconnectAttempts++;
    const delay = this.options.reconnectDelayMs * Math.pow(2, this.reconnectAttempts - 1);
    const cappedDelay = Math.min(delay, 30000); // Cap at 30s

    setTimeout(() => {
      if (this.state === 'disconnected' && !this.intentionalClose) {
        this.connect().catch(() => {
          // Reconnect failed; handleClose will schedule another attempt
        });
      }
    }, cappedDelay);
  }

  // ── Keepalive ───────────────────────────────────────────────────────

  private startKeepalive(): void {
    this.stopKeepalive();
    this.pingTimer = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send('__ping__');
        this.resetPongTimer();
      }
    }, this.options.pingIntervalMs);
  }

  private stopKeepalive(): void {
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
    this.resetPongTimer();
  }

  private resetPongTimer(): void {
    if (this.pongTimer !== null) {
      clearTimeout(this.pongTimer);
      this.pongTimer = null;
    }

    if (this.state === 'connected') {
      this.pongTimer = setTimeout(() => {
        // Pong timeout -- close the connection so reconnect can happen
        if (this.ws?.readyState === WebSocket.OPEN) {
          this.ws.close(4000, 'Pong timeout');
        }
      }, this.options.pongTimeoutMs);
    }
  }

  // ── Queue ───────────────────────────────────────────────────────────

  private drainQueue(): void {
    while (this.messageQueue.length > 0) {
      const msg = this.messageQueue.shift()!;
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(typeof msg === 'string' ? msg : JSON.stringify(msg));
      } else {
        // Connection lost during drain -- put back
        this.messageQueue.unshift(msg);
        break;
      }
    }
  }

  // ── Emitters ────────────────────────────────────────────────────────

  private emitMessage(chunk: ChatChunk): void {
    for (const fn of this.messageListeners) {
      try { fn(chunk); } catch { /* swallow */ }
    }
  }

  private emitError(error: WSError): void {
    for (const fn of this.errorListeners) {
      try { fn(error); } catch { /* swallow */ }
    }
  }

  private emitOpen(): void {
    for (const fn of this.openListeners) {
      try { fn(); } catch { /* swallow */ }
    }
  }

  private emitClose(event: CloseEvent): void {
    for (const fn of this.closeListeners) {
      try { fn(event); } catch { /* swallow */ }
    }
  }
}
