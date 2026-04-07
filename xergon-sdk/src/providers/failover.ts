/**
 * Multi-provider failover with circuit breaker pattern.
 *
 * Provides automatic failover across multiple relay endpoints with:
 * - Priority-based endpoint selection
 * - Circuit breaker (marks unhealthy after N consecutive failures)
 * - Background health checks to recover unhealthy endpoints
 * - Streaming support with failover
 */

// ── Types ────────────────────────────────────────────────────────────

export interface ProviderEndpoint {
  url: string;
  priority: number; // lower = higher priority
  region: string;
  maxRetries: number; // default 3
  timeoutMs: number; // default 30000
}

export interface FailoverOptions {
  healthCheckIntervalMs?: number; // default 30000
  healthCheckTimeoutMs?: number; // default 5000
  circuitBreakerThreshold?: number; // default 3 consecutive failures
  circuitBreakerResetMs?: number; // default 60000
  retryDelayMs?: number; // default 1000
  retryBackoffFactor?: number; // default 2 (exponential)
  /** Custom fetch implementation (useful for tests). */
  fetchFn?: typeof fetch;
  /** Custom headers to include on every request. */
  headers?: Record<string, string>;
}

export interface EndpointHealth {
  url: string;
  healthy: boolean;
  latencyMs: number;
  lastCheckedAt: number;
  consecutiveFailures: number;
  lastError?: string;
}

export interface EndpointStatus extends ProviderEndpoint {
  healthy: boolean;
  latencyMs: number;
  lastCheckedAt: number;
  consecutiveFailures: number;
  lastError?: string;
}

export interface ServerSentEvent {
  data: string;
  event?: string;
  id?: string;
}

// ── Defaults ─────────────────────────────────────────────────────────

const DEFAULTS = {
  healthCheckIntervalMs: 30_000,
  healthCheckTimeoutMs: 5_000,
  circuitBreakerThreshold: 3,
  circuitBreakerResetMs: 60_000,
  retryDelayMs: 1_000,
  retryBackoffFactor: 2,
  maxRetries: 3,
  timeoutMs: 30_000,
};

// ── Internal State ───────────────────────────────────────────────────

interface EndpointState {
  endpoint: ProviderEndpoint;
  healthy: boolean;
  latencyMs: number;
  lastCheckedAt: number;
  consecutiveFailures: number;
  lastError?: string;
  circuitOpenAt?: number; // when the circuit was opened
}

// ── Error Classes ────────────────────────────────────────────────────

export class AllEndpointsFailedError extends Error {
  public readonly errors: Array<{ url: string; error: unknown }>;

  constructor(errors: Array<{ url: string; error: unknown }>) {
    const summary = errors
      .map((e) => `${e.url}: ${e.error instanceof Error ? e.error.message : String(e.error)}`)
      .join('; ');
    super(`All endpoints failed: ${summary}`);
    this.name = 'AllEndpointsFailedError';
    this.errors = errors;
  }
}

// ── FailoverProviderManager ──────────────────────────────────────────

export class FailoverProviderManager {
  private states: Map<string, EndpointState>;
  private options: Required<FailoverOptions> & { fetchFn: typeof fetch; headers: Record<string, string> };
  private healthCheckTimer: ReturnType<typeof setInterval> | null = null;

  constructor(endpoints: ProviderEndpoint[], options?: FailoverOptions) {
    this.options = {
      healthCheckIntervalMs: options?.healthCheckIntervalMs ?? DEFAULTS.healthCheckIntervalMs,
      healthCheckTimeoutMs: options?.healthCheckTimeoutMs ?? DEFAULTS.healthCheckTimeoutMs,
      circuitBreakerThreshold: options?.circuitBreakerThreshold ?? DEFAULTS.circuitBreakerThreshold,
      circuitBreakerResetMs: options?.circuitBreakerResetMs ?? DEFAULTS.circuitBreakerResetMs,
      retryDelayMs: options?.retryDelayMs ?? DEFAULTS.retryDelayMs,
      retryBackoffFactor: options?.retryBackoffFactor ?? DEFAULTS.retryBackoffFactor,
      fetchFn: options?.fetchFn ?? fetch,
      headers: options?.headers ?? {},
    };

    this.states = new Map();
    for (const ep of endpoints) {
      this.states.set(ep.url, {
        endpoint: { ...ep, maxRetries: ep.maxRetries ?? DEFAULTS.maxRetries, timeoutMs: ep.timeoutMs ?? DEFAULTS.timeoutMs },
        healthy: true,
        latencyMs: 0,
        lastCheckedAt: 0,
        consecutiveFailures: 0,
      });
    }

    // Start background health checks
    this.startHealthCheckLoop();
  }

  // ── Endpoint Selection ─────────────────────────────────────────────

  /**
   * Get the best available endpoint (healthy + lowest priority number).
   */
  async getBestEndpoint(): Promise<ProviderEndpoint> {
    const candidates = this.getHealthyEndpoints();
    if (candidates.length === 0) {
      // Try a fresh health check before giving up
      await this.healthCheck();
      const refreshed = this.getHealthyEndpoints();
      if (refreshed.length === 0) {
        throw new AllEndpointsFailedError(
          Array.from(this.states.values()).map((s) => ({
            url: s.endpoint.url,
            error: s.lastError ?? 'unhealthy',
          })),
        );
      }
      return refreshed[0].endpoint;
    }
    return candidates[0].endpoint;
  }

  private getHealthyEndpoints(): EndpointState[] {
    return Array.from(this.states.values())
      .filter((s) => s.healthy)
      .sort((a, b) => a.endpoint.priority - b.endpoint.priority || a.latencyMs - b.latencyMs);
  }

  /**
   * Get all endpoints sorted by priority (healthy first, then unhealthy).
   */
  getSortedEndpoints(): EndpointState[] {
    return Array.from(this.states.values()).sort((a, b) => {
      if (a.healthy !== b.healthy) return a.healthy ? -1 : 1;
      return a.endpoint.priority - b.endpoint.priority;
    });
  }

  // ── Request with Failover ──────────────────────────────────────────

  /**
   * Execute a request with automatic failover across endpoints.
   */
  async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const errors: Array<{ url: string; error: unknown }> = [];
    const sorted = this.getSortedEndpoints();

    for (const state of sorted) {
      const { endpoint } = state;

      // Skip circuit-open endpoints (unless reset window has elapsed)
      if (!state.healthy && state.circuitOpenAt) {
        const elapsed = Date.now() - state.circuitOpenAt;
        if (elapsed < this.options.circuitBreakerResetMs) {
          errors.push({ url: endpoint.url, error: `circuit open (${Math.round(elapsed)}ms)` });
          continue;
        }
        // Half-open: try the endpoint
      }

      try {
        const result = await this.executeRequest<T>(endpoint, method, path, body);
        this.recordSuccess(state);
        return result;
      } catch (err) {
        errors.push({ url: endpoint.url, error: err });
        this.recordFailure(state, err);
      }
    }

    throw new AllEndpointsFailedError(errors);
  }

  /**
   * Stream with failover (for SSE). Returns an AsyncIterable of ServerSentEvent.
   */
  async *stream(path: string, body?: unknown): AsyncIterable<ServerSentEvent> {
    const sorted = this.getSortedEndpoints();

    for (const state of sorted) {
      const { endpoint } = state;

      if (!state.healthy && state.circuitOpenAt) {
        const elapsed = Date.now() - state.circuitOpenAt;
        if (elapsed < this.options.circuitBreakerResetMs) continue;
      }

      try {
        const iterator = this.executeStreamRequest(endpoint, path, body)[Symbol.asyncIterator]();
        // Consume one chunk to verify the connection is alive
        const firstResult = await iterator.next();
        if (firstResult.done) {
          this.recordFailure(state, 'empty stream');
          continue;
        }

        this.recordSuccess(state);
        yield firstResult.value;

        // Yield remaining chunks
        let next: IteratorResult<ServerSentEvent, unknown>;
        while (true) {
          next = await iterator.next();
          if (next.done) break;
          yield next.value;
        }
        return; // Stream completed successfully
      } catch (err) {
        this.recordFailure(state, err);
      }
    }

    throw new AllEndpointsFailedError(
      Array.from(this.states.values()).map((s) => ({
        url: s.endpoint.url,
        error: s.lastError ?? 'stream failed',
      })),
    );
  }

  // ── Health Checks ──────────────────────────────────────────────────

  /**
   * Health check all endpoints and return results.
   */
  async healthCheck(): Promise<Map<string, EndpointHealth>> {
    const results = new Map<string, EndpointHealth>();
    const checks = Array.from(this.states.entries()).map(async ([url, state]) => {
      const health = await this.checkEndpoint(state);
      results.set(url, health);
    });
    await Promise.allSettled(checks);
    return results;
  }

  /**
   * Get all endpoints with their health status.
   */
  getEndpoints(): EndpointStatus[] {
    return Array.from(this.states.values()).map((s) => ({
      url: s.endpoint.url,
      priority: s.endpoint.priority,
      region: s.endpoint.region,
      maxRetries: s.endpoint.maxRetries,
      timeoutMs: s.endpoint.timeoutMs,
      healthy: s.healthy,
      latencyMs: s.latencyMs,
      lastCheckedAt: s.lastCheckedAt,
      consecutiveFailures: s.consecutiveFailures,
      lastError: s.lastError,
    }));
  }

  /**
   * Manually mark an endpoint as unhealthy.
   */
  markUnhealthy(url: string, reason: string): void {
    const state = this.states.get(url);
    if (state) {
      state.healthy = false;
      state.consecutiveFailures = this.options.circuitBreakerThreshold;
      state.lastError = reason;
      state.circuitOpenAt = Date.now();
    }
  }

  /**
   * Reset all endpoints to healthy.
   */
  resetAll(): void {
    for (const state of this.states.values()) {
      state.healthy = true;
      state.consecutiveFailures = 0;
      state.lastError = undefined;
      state.circuitOpenAt = undefined;
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────

  /**
   * Stop background health checks and clean up resources.
   */
  destroy(): void {
    if (this.healthCheckTimer !== null) {
      clearInterval(this.healthCheckTimer);
      this.healthCheckTimer = null;
    }
  }

  // ── Private Methods ────────────────────────────────────────────────

  private async checkEndpoint(state: EndpointState): Promise<EndpointHealth> {
    const { endpoint } = state;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.options.healthCheckTimeoutMs);

    try {
      const start = Date.now();
      const res = await this.options.fetchFn(`${endpoint.url}/health`, {
        method: 'GET',
        signal: controller.signal,
        headers: this.options.headers,
      });
      const latencyMs = Date.now() - start;

      state.lastCheckedAt = Date.now();
      state.latencyMs = latencyMs;

      if (res.ok) {
        state.healthy = true;
        state.consecutiveFailures = 0;
        state.lastError = undefined;
        state.circuitOpenAt = undefined;
      } else {
        this.recordFailure(state, `health check returned ${res.status}`);
      }
    } catch (err) {
      state.lastCheckedAt = Date.now();
      this.recordFailure(state, err);
    } finally {
      clearTimeout(timeout);
    }

    return {
      url: endpoint.url,
      healthy: state.healthy,
      latencyMs: state.latencyMs,
      lastCheckedAt: state.lastCheckedAt,
      consecutiveFailures: state.consecutiveFailures,
      lastError: state.lastError,
    };
  }

  private recordSuccess(state: EndpointState): void {
    state.consecutiveFailures = 0;
    state.healthy = true;
    state.lastError = undefined;
    state.circuitOpenAt = undefined;
  }

  private recordFailure(state: EndpointState, err: unknown): void {
    state.consecutiveFailures += 1;
    state.lastError = err instanceof Error ? err.message : String(err);
    state.lastCheckedAt = Date.now();

    if (state.consecutiveFailures >= this.options.circuitBreakerThreshold) {
      state.healthy = false;
      state.circuitOpenAt = Date.now();
    }
  }

  private async executeRequest<T>(
    endpoint: ProviderEndpoint,
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), endpoint.timeoutMs);

    try {
      const url = `${endpoint.url}${path}`;
      const headers: Record<string, string> = {
        ...this.options.headers,
      };

      if (body !== undefined) {
        headers['Content-Type'] = 'application/json';
      }

      const res = await this.options.fetchFn(url, {
        method,
        headers,
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      if (!res.ok) {
        let errorData: unknown;
        try {
          errorData = await res.json();
        } catch {
          errorData = { message: res.statusText };
        }
        const errMsg =
          errorData && typeof errorData === 'object' && 'message' in errorData
            ? String((errorData as Record<string, unknown>).message)
            : `HTTP ${res.status}`;
        throw new Error(errMsg);
      }

      const contentType = res.headers.get('content-type');
      if (!contentType || contentType === 'text/plain' || res.status === 204) {
        const text = await res.text();
        return text as T;
      }

      return (await res.json()) as T;
    } finally {
      clearTimeout(timeout);
    }
  }

  private executeStreamRequest(
    endpoint: ProviderEndpoint,
    path: string,
    body?: unknown,
  ): AsyncIterable<ServerSentEvent> {
    const self = this;
    const url = `${endpoint.url}${path}`;
    const headers: Record<string, string> = {
      ...this.options.headers,
      'Content-Type': 'application/json',
      Accept: 'text/event-stream',
    };

    async function* iterate(): AsyncGenerator<ServerSentEvent> {
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), endpoint.timeoutMs);

      try {
        const res = await self.options.fetchFn(url, {
          method: 'POST',
          headers,
          body: body !== undefined ? JSON.stringify(body) : undefined,
          signal: controller.signal,
        });

        if (!res.ok) {
          throw new Error(`HTTP ${res.status}: ${res.statusText}`);
        }

        if (!res.body) {
          throw new Error('Response body is null');
        }

        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const events = parseSSEBuffer(buffer);
          buffer = events.remaining;

          for (const event of events.parsed) {
            yield event;
          }
        }

        // Process any remaining data
        if (buffer.trim()) {
          const events = parseSSEBuffer(buffer + '\n\n');
          for (const event of events.parsed) {
            yield event;
          }
        }
      } finally {
        clearTimeout(timeout);
      }
    }

    return iterate();
  }

  private startHealthCheckLoop(): void {
    if (this.options.healthCheckIntervalMs <= 0) return;

    this.healthCheckTimer = setInterval(async () => {
      try {
        await this.healthCheck();
      } catch {
        // Health check errors are silently handled per-endpoint
      }
    }, this.options.healthCheckIntervalMs);

    // Allow the timer to not prevent process exit
    if (typeof this.healthCheckTimer === 'object' && this.healthCheckTimer !== null && 'unref' in this.healthCheckTimer) {
      (this.healthCheckTimer as unknown as { unref: () => void }).unref();
    }
  }
}

// ── SSE Parsing ──────────────────────────────────────────────────────

function parseSSEBuffer(buffer: string): { parsed: ServerSentEvent[]; remaining: string } {
  const events: ServerSentEvent[] = [];
  const lines = buffer.split('\n');
  let currentEvent: Partial<ServerSentEvent> = {};
  let eventLines: string[] = [];
  let lastCompleteIndex = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (line === '' || (line === '\r' && lines[i - 1] !== '')) {
      // Empty line = end of event
      if (eventLines.length > 0) {
        events.push({
          event: currentEvent.event,
          data: currentEvent.data ?? '',
          id: currentEvent.id,
        });
        currentEvent = {};
        eventLines = [];
        lastCompleteIndex = i + 1;
      }
      continue;
    }

    eventLines.push(line);

    if (line.startsWith('data:')) {
      const data = line.slice(5).trimStart();
      currentEvent.data = currentEvent.data ? currentEvent.data + '\n' + data : data;
    } else if (line.startsWith('event:')) {
      currentEvent.event = line.slice(6).trimStart();
    } else if (line.startsWith('id:')) {
      currentEvent.id = line.slice(3).trimStart();
    }
  }

  return {
    parsed: events,
    remaining: lastCompleteIndex < lines.length ? lines.slice(lastCompleteIndex).join('\n') : '',
  };
}
