/**
 * Resilient HTTP client combining retry, cancellation, and timeout.
 *
 * Wraps fetch with:
 * - Automatic retry on retryable errors (via RetryClient)
 * - Request cancellation (via CancellationToken)
 * - Per-request timeouts
 * - Request deduplication (via RequestQueue)
 */

import { RetryClient } from './retry';
import type { RetryConfig, BackoffStrategy } from './retry';
import { CancellationToken, CancellationManager } from './cancellation';
import { RequestQueue } from './queue';

// ── Types ────────────────────────────────────────────────────────────

export interface ResilientOptions {
  /** Retry configuration. */
  retry?: Partial<RetryConfig>;
  /** Default timeout per request in ms (default: 30000). */
  timeoutMs?: number;
  /** Default headers included in every request. */
  headers?: Record<string, string>;
  /** Enable request deduplication (default: true). */
  deduplicate?: boolean;
}

export interface RequestOptions {
  /** Query parameters to append to the URL. */
  params?: Record<string, string>;
  /** Additional headers for this request. */
  headers?: Record<string, string>;
  /** External AbortSignal for cancellation. */
  signal?: AbortSignal;
  /** Per-request timeout override in ms. */
  timeoutMs?: number;
  /** Backoff strategy override for retries. */
  retryStrategy?: BackoffStrategy;
  /** Disable retry for this specific request. */
  noRetry?: boolean;
}

export interface RequestConfig {
  /** HTTP method. */
  method: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  /** URL path (appended to baseURL). */
  path: string;
  /** Request body (will be JSON-serialized for POST/PUT/PATCH). */
  body?: unknown;
  /** Query parameters. */
  params?: Record<string, string>;
  /** Additional headers. */
  headers?: Record<string, string>;
  /** External AbortSignal. */
  signal?: AbortSignal;
  /** Per-request timeout. */
  timeoutMs?: number;
  /** Backoff strategy. */
  retryStrategy?: BackoffStrategy;
  /** Disable retry. */
  noRetry?: boolean;
}

// ── ResilientHttpClient ──────────────────────────────────────────────

export class ResilientHttpClient {
  private readonly baseURL: string;
  private readonly retryClient: RetryClient;
  private readonly cancellation: CancellationManager;
  private readonly queue: RequestQueue;
  private readonly defaultHeaders: Record<string, string>;
  private readonly defaultTimeoutMs: number;
  private readonly deduplicate: boolean;

  constructor(baseURL: string, options?: ResilientOptions) {
    this.baseURL = baseURL.replace(/\/+$/, '');
    this.retryClient = new RetryClient(options?.retry);
    this.cancellation = new CancellationManager();
    this.queue = new RequestQueue();
    this.defaultHeaders = options?.headers ?? {};
    this.defaultTimeoutMs = options?.timeoutMs ?? 30000;
    this.deduplicate = options?.deduplicate ?? true;
  }

  /** Access the underlying RetryClient. */
  get retry(): RetryClient {
    return this.retryClient;
  }

  /** Access the CancellationManager. */
  get cancellationManager(): CancellationManager {
    return this.cancellation;
  }

  // ── Convenience Methods ─────────────────────────────────────────────

  /**
   * Perform a GET request with retry and cancellation support.
   */
  async get<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>({ method: 'GET', path, ...options });
  }

  /**
   * Perform a POST request with retry and cancellation support.
   */
  async post<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>({ method: 'POST', path, body, ...options });
  }

  /**
   * Perform a PUT request with retry and cancellation support.
   */
  async put<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>({ method: 'PUT', path, body, ...options });
  }

  /**
   * Perform a DELETE request with retry and cancellation support.
   */
  async delete<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>({ method: 'DELETE', path, ...options });
  }

  /**
   * Perform a PATCH request with retry and cancellation support.
   */
  async patch<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>({ method: 'PATCH', path, body, ...options });
  }

  // ── Core Request ───────────────────────────────────────────────────

  /**
   * Perform a request with full control over method, path, body, etc.
   * Automatically applies retry, timeout, and cancellation.
   */
  async request<T>(config: RequestConfig): Promise<T> {
    const { method, path, body, params, headers, signal, timeoutMs, retryStrategy, noRetry } = config;

    const effectiveTimeout = timeoutMs ?? this.defaultTimeoutMs;

    // Build URL with query params
    let url = `${this.baseURL}${path}`;
    if (params && Object.keys(params).length > 0) {
      const searchParams = new URLSearchParams(params);
      const separator = url.includes('?') ? '&' : '?';
      url += `${separator}${searchParams.toString()}`;
    }

    // Merge headers
    const mergedHeaders: Record<string, string> = {
      ...this.defaultHeaders,
      ...headers,
    };

    if (body !== undefined && !mergedHeaders['Content-Type']) {
      mergedHeaders['Content-Type'] = 'application/json';
    }

    // Create dedup key for GET requests
    const dedupKey: string | null = this.deduplicate && method === 'GET'
      ? `${method}:${url}:${JSON.stringify(mergedHeaders)}`
      : null;

    const executeRequest = async (): Promise<T> => {
      // Create a token for this request
      const token = this.cancellation.createToken();

      // Create timeout token
      const timeoutToken = token.withTimeout(effectiveTimeout);

      // Link external signal if provided
      if (signal && !signal.aborted) {
        // Listen for external abort
        const onExternalAbort = () => {
          token.cancel('External abort signal triggered');
        };
        signal.addEventListener('abort', onExternalAbort, { once: true });
        timeoutToken.signal.addEventListener('abort', () => {
          signal.removeEventListener('abort', onExternalAbort);
        }, { once: true });
      } else if (signal?.aborted) {
        throw new DOMException('Operation aborted', 'AbortError');
      }

      const doFetch = async (abortSignal: AbortSignal): Promise<T> => {
        const res = await fetch(url, {
          method,
          headers: mergedHeaders,
          body: body !== undefined ? JSON.stringify(body) : undefined,
          signal: abortSignal,
        });

        if (!res.ok) {
          let errorData: unknown;
          try {
            errorData = await res.json();
          } catch {
            errorData = { message: res.statusText };
          }

          // Extract message from nested or flat error format
          let message: string;
          if (
            errorData &&
            typeof errorData === 'object' &&
            'error' in errorData &&
            typeof (errorData as Record<string, unknown>).error === 'object' &&
            (errorData as Record<string, unknown>).error !== null
          ) {
            const inner = (errorData as { error: Record<string, unknown> }).error;
            message = typeof inner.message === 'string' ? inner.message : res.statusText;
          } else if (
            errorData &&
            typeof errorData === 'object' &&
            'message' in errorData &&
            typeof (errorData as Record<string, unknown>).message === 'string'
          ) {
            message = (errorData as Record<string, unknown>).message as string;
          } else {
            message = res.statusText;
          }

          const err = new Error(message);
          Object.assign(err, { code: res.status });
          throw err;
        }

        // Handle empty responses
        const contentType = res.headers.get('content-type');
        if (!contentType || contentType === 'text/plain' || res.status === 204) {
          const text = await res.text();
          return text as T;
        }

        return (await res.json()) as T;
      };

      // Use retry or direct execution
      if (noRetry) {
        return doFetch(timeoutToken.signal);
      }

      return this.retryClient.execute(doFetch, {
        strategy: retryStrategy,
        signal: timeoutToken.signal,
      });
    };

    // Use queue for deduplication on GET requests
    if (dedupKey !== null) {
      return this.queue.enqueue(dedupKey, executeRequest, effectiveTimeout);
    }

    return executeRequest();
  }

  // ── Cancellable Request ─────────────────────────────────────────────

  /**
   * Create a cancellable request that returns both the promise and a cancel function.
   *
   * @example
   * ```ts
   * const { promise, cancel } = client.cancellableRequest({
   *   method: 'GET',
   *   path: '/v1/models',
   * });
   *
   * // Later, if you want to cancel:
   * cancel();
   * ```
   */
  cancellableRequest<T>(config: RequestConfig): {
    promise: Promise<T>;
    cancel: (reason?: string) => void;
  } {
    const token = this.cancellation.createToken();

    const mergedConfig: RequestConfig = {
      ...config,
      signal: token.signal,
    };

    return {
      promise: this.request<T>(mergedConfig),
      cancel: (reason?: string) => token.cancel(reason),
    };
  }

  // ── Lifecycle ───────────────────────────────────────────────────────

  /**
   * Cancel all active requests.
   */
  cancelAll(reason?: string): void {
    this.cancellation.cancelAll(reason);
    this.queue.flush();
  }

  /**
   * Get retry statistics.
   */
  getRetryStats() {
    return this.retryClient.getStats();
  }

  /**
   * Reset retry statistics.
   */
  resetRetryStats(): void {
    this.retryClient.resetStats();
  }

  /**
   * Dispose of all resources.
   */
  dispose(): void {
    this.cancelAll('ResilientHttpClient disposed');
    this.cancellation.dispose();
  }
}
