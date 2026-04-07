/**
 * Batch request execution -- parallel and sequential with concurrency control.
 *
 * Provides BatchClient for executing multiple HTTP requests in a single
 * batch call, with per-request timeout, error isolation, and ID correlation.
 */

// ── Types ────────────────────────────────────────────────────────────

export interface BatchRequestItem {
  /** Client-generated request ID for correlation. */
  id: string;
  method: 'POST' | 'GET';
  /** API path, e.g. '/v1/chat/completions' */
  path: string;
  body?: any;
  headers?: Record<string, string>;
}

export interface BatchResponseItem {
  /** Matches the request ID. */
  id: string;
  status: number;
  headers: Record<string, string>;
  body: any;
  /** Time taken for this individual request in milliseconds. */
  duration_ms: number;
  /** Error message if the request failed. */
  error?: string;
}

export interface BatchRequest {
  requests: BatchRequestItem[];
  /** Execute requests sequentially instead of in parallel. Default: false. */
  sequential?: boolean;
  /** Per-request timeout in milliseconds. Default: 30000. */
  timeoutMs?: number;
}

export interface BatchResponse {
  responses: BatchResponseItem[];
  /** Total wall-clock time for the entire batch in milliseconds. */
  total_duration_ms: number;
}

// ── Semaphore ────────────────────────────────────────────────────────

class Semaphore {
  private queue: Array<() => void> = [];
  private running = 0;

  constructor(private max: number) {}

  async acquire(): Promise<void> {
    if (this.running < this.max) {
      this.running++;
      return;
    }
    return new Promise<void>((resolve) => {
      this.queue.push(() => {
        this.running++;
        resolve();
      });
    });
  }

  release(): void {
    this.running--;
    if (this.queue.length > 0) {
      const next = this.queue.shift()!;
      next();
    }
  }
}

// ── BatchClient ──────────────────────────────────────────────────────

export class BatchClient {
  /** Maximum concurrent requests in parallel mode. Default: 10. */
  maxConcurrent: number;

  private baseURL: string;
  private defaultHeaders: Record<string, string>;

  constructor(baseURL: string, defaultHeaders?: Record<string, string>) {
    this.baseURL = baseURL.replace(/\/+$/, '');
    this.defaultHeaders = defaultHeaders ?? {};
    this.maxConcurrent = 10;
  }

  /**
   * Execute a batch of requests.
   *
   * In parallel mode (default), requests run concurrently up to `maxConcurrent`.
   * In sequential mode, requests execute one after another in order.
   * Errors are isolated -- a failure in one request does not stop others.
   */
  async execute(batch: BatchRequest): Promise<BatchResponse> {
    const startTime = Date.now();
    const timeoutMs = batch.timeoutMs ?? 30000;

    if (batch.sequential) {
      const responses: BatchResponseItem[] = [];
      for (const req of batch.requests) {
        responses.push(await this.executeOne(req, timeoutMs));
      }
      return {
        responses,
        total_duration_ms: Date.now() - startTime,
      };
    }

    // Parallel with concurrency limit
    const semaphore = new Semaphore(this.maxConcurrent);
    const promises = batch.requests.map(async (req) => {
      await semaphore.acquire();
      try {
        return await this.executeOne(req, timeoutMs);
      } finally {
        semaphore.release();
      }
    });

    const responses = await Promise.all(promises);
    return {
      responses,
      total_duration_ms: Date.now() - startTime,
    };
  }

  /**
   * Convenience: batch chat completions.
   * Each request in the array should include at least `model` and `messages`.
   */
  async chatBatch(
    requests: Array<{ model: string; messages: any[]; [key: string]: any }>,
  ): Promise<BatchResponse> {
    const batch: BatchRequest = {
      requests: requests.map((req, i) => ({
        id: req.id ?? `chat-${i}`,
        method: 'POST' as const,
        path: '/v1/chat/completions',
        body: { ...req, stream: false },
      })),
    };
    return this.execute(batch);
  }

  /**
   * Convenience: batch model info lookups.
   * Sends GET requests for multiple model IDs (or the same list endpoint).
   */
  async modelInfoBatch(modelIds: string[]): Promise<BatchResponse> {
    const batch: BatchRequest = {
      requests: modelIds.map((id, i) => ({
        id: `model-${i}`,
        method: 'GET' as const,
        path: `/v1/models/${encodeURIComponent(id)}`,
      })),
    };
    return this.execute(batch);
  }

  // ── Private ─────────────────────────────────────────────────────────

  private async executeOne(
    req: BatchRequestItem,
    timeoutMs: number,
  ): Promise<BatchResponseItem> {
    const startTime = Date.now();

    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), timeoutMs);

      const url = `${this.baseURL}${req.path}`;
      const headers: Record<string, string> = {
        ...this.defaultHeaders,
        ...req.headers,
      };

      if (req.method === 'POST' && req.body !== undefined) {
        headers['Content-Type'] = 'application/json';
      }

      const res = await fetch(url, {
        method: req.method,
        headers,
        body: req.method === 'POST' && req.body !== undefined
          ? JSON.stringify(req.body)
          : undefined,
        signal: controller.signal,
      });

      clearTimeout(timer);

      // Extract response headers
      const resHeaders: Record<string, string> = {};
      res.headers.forEach((value, key) => {
        resHeaders[key] = value;
      });

      // Parse body
      let body: any;
      const ct = res.headers.get('content-type') ?? '';
      if (ct.includes('application/json')) {
        body = await res.json();
      } else {
        body = await res.text();
      }

      return {
        id: req.id,
        status: res.status,
        headers: resHeaders,
        body,
        duration_ms: Date.now() - startTime,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        id: req.id,
        status: 0,
        headers: {},
        body: null,
        duration_ms: Date.now() - startTime,
        error: message,
      };
    }
  }
}
